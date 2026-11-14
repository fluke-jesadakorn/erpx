import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeCustomerTimelineClient,
  nativeCustomerTimelineAvailable,
  nativeCustomerTimelineConfiguration,
  NativeCustomerTimelineError,
  NATIVE_CUSTOMER_TIMELINE_LIMITATIONS,
  type NativeCustomerTimelinePageContext,
} from '../../../packages/feature-sales/src/public/native-customer-timeline';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44996',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeCustomerTimelinePageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'support_agent',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['field_service'],
  iam:{permissions:['commerce.read','support.read','finance.read','legal.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'support_agent',assignmentScopeKind:'holding',
  modules:['field_service'],
  permissions:['commerce.read','support.read','finance.read','legal.read'],
  expiresAt:9_007_199_254_740_991};
const customer={id:'customer',name:'Acme Customer',code:'C-001'};
const order={
  id:'order:order-1',source_id:'order-1',source_type:'order',title:'sales · SO-100',
  status:'confirmed',version:1,at:100,href:'/commerce/orders/order-1',
};
const coverage=[
  'Sales quotations/orders',
  'Fulfillments and customer returns',
  'Internal contact history',
  'Customer follow-up history',
  'Qualified CRM lead/opportunity history',
  'CRM follow-up activities',
  'Support case history',
  'Field Service agreements and visits',
  'Finance invoice status',
  'Legal records linked to orders',
];
const page=(items:unknown[],overrides:Record<string,unknown>={})=>({
  data:{customer,items,hasMore:false,nextCursor:null,coverage,
    limitations:NATIVE_CUSTOMER_TIMELINE_LIMITATIONS,...overrides},
  context,
});
const input={holdingSlug:'acme',customerId:'customer',cursor:'opaque',limit:30,
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Customer 360 timeline transport',()=>{
 it('accepts one bounded contributor Holding port and prefers the forwarded host',async()=>{
  const contributor={...JSON.parse(config()),holdings:[{origin:'https://acme.example.com:3443',holdingSlug:'acme',holdingId:'h001'}]};
  const transport=vi.fn().mockResolvedValue(ok(page([order])));
  const client=createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(JSON.stringify(contributor)),transport);
  await client.timeline(incoming({host:'127.0.0.1:3207','x-forwarded-host':'acme.example.com:3443'}),actor,input);
  expect(transport.mock.calls[0][2].get('host')).toBe('acme.example.com:3443');
  for(const origin of ['https://acme.example.com:0','https://acme.example.com:65536','https://acme.example.com:abc'])
    expect(()=>createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config({holdings:[{origin,holdingSlug:'acme',holdingId:'h001'}]})),vi.fn())).toThrow();
 });

 it('forwards the configured Holding, customer, session, CAS, cursor and limit',async()=>{
  const transport=vi.fn().mockResolvedValue(ok(page([order])));
  const client=createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config()),transport);
  const result=await client.timeline(incoming(),actor,input);
  expect(result).toEqual({customer,items:[order],hasMore:false,nextCursor:null,
    coverage,limitations:NATIVE_CUSTOMER_TIMELINE_LIMITATIONS});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/customers/customer/timeline?cursor=opaque&limit=30');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug and a bounded page',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:{
    customer,items:[order],hasMore:true,nextCursor:'next',coverage:[coverage[0]],
    limitations:NATIVE_CUSTOMER_TIMELINE_LIMITATIONS,
  },context:branchContext}));
  const client=createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config()),transport);
  const result=await client.timeline(incoming(),branchActor,{...input,branchSlug:'north',cursor:undefined,limit:1});
  expect(result.nextCursor).toBe('next');
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/customers/customer/timeline?limit=1');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'PACKAGE_REQUIRED',message:'Support history is required.'}}),
    {status:403,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config()),transport);
  await expect(client.timeline(incoming(),actor,input)).rejects.toMatchObject({
    code:'PACKAGE_REQUIRED',status:403,message:'Support history is required.',
  });
 });

 it('rejects scope, item, coverage, page and context mismatches before returning data',async()=>{
  const client=(value:unknown)=>createNativeCustomerTimelineClient(
    nativeCustomerTimelineConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  for(const invalid of [
    {...order,id:'invoice:order-1'},
    {...order,source_type:'portal'},
    {...order,version:0},
    {...order,at:-1},
    {...order,href:'https://evil.example/path'},
    {...order,href:'//evil.example/path'},
    {...order,title:''},
    {...order,status:'bad\u0007status'},
    {...order,extra:'field'},
  ]) {
    await expect(client(page([invalid])).timeline(incoming(),actor,input))
      .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  }
  await expect(client(page([order,order])).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{hasMore:true})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{nextCursor:'next'})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{coverage:[coverage[0],coverage[0]]})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{coverage:['Portal actions']})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{limitations:'Other.'})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{customer:{...customer,id:'other'}})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client(page([order],{customer:{...customer,code:''}})).timeline(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID'});
  await expect(client({data:page([order]).data,context:{...context,branchId:'branch',branchSlug:'north'}})
    .timeline(incoming(),actor,input)).rejects.toMatchObject({
      code:'NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED',
    });
  await expect(client({data:page([order]).data,context:{...context,revision:8}})
    .timeline(incoming(),actor,input)).rejects.toMatchObject({
      code:'NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED',
    });
  await expect(client({data:page([order]).data,context:{...context,permissions:['commerce.read']}})
    .timeline(incoming(),actor,input)).rejects.toMatchObject({
      code:'NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED',
    });
 });

 it('rejects non-monotonic, oversized and duplicate pages',async()=>{
  const client=(items:unknown[])=>createNativeCustomerTimelineClient(
    nativeCustomerTimelineConfiguration(config()),
    vi.fn().mockResolvedValue(ok(page(items))),
  );
  await expect(client([
    {...order,id:'order:order-2',source_id:'order-2',at:101},
    {...order,at:101},
  ]).timeline(incoming(),actor,input)).resolves.toBeDefined();
  await expect(client([
    {...order,at:100},
    {...order,id:'order:order-2',source_id:'order-2',at:101},
  ]).timeline(incoming(),actor,input)).rejects.toMatchObject({
    code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID',
  });
  await expect(client([
    {...order,id:'order:order-0',source_id:'order-0',at:100},
    {...order,at:100},
  ]).timeline(incoming(),actor,input)).rejects.toMatchObject({
    code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID',
  });
  await expect(client([
    {...order,at:100},
    {...order,id:'contact:contact-1',source_id:'contact-1',source_type:'contact',at:99,href:'/commerce/orders/order-1/notes'},
  ]).timeline(incoming(),actor,{...input,limit:1})).rejects.toMatchObject({
    code:'NATIVE_CUSTOMER_TIMELINE_RESPONSE_INVALID',
  });
 });

 it('accepts foreign source families only with the exact source prefix',async()=>{
  const items=[
    {...order,id:'support:ticket-1',source_id:'ticket-1',source_type:'support',href:'/support/ticket-1'},
    {...order,id:'service-agreement:agreement-1',source_id:'agreement-1',source_type:'field_service',href:'/field-service'},
    {...order,id:'service-order:order-9',source_id:'order-9',source_type:'field_service',at:99,href:'/field-service/order-9'},
    {...order,id:'legal:legal-1',source_id:'legal-1',source_type:'legal',at:98,href:'/legal'},
    {...order,id:'invoice:invoice-1',source_id:'invoice-1',source_type:'invoice',at:97,href:'/finance/invoices/invoice-1'},
    {...order,id:'crm:lead-1',source_id:'lead-1',source_type:'crm',at:96,href:'/crm/lead-1'},
    {...order,id:'crm-activity:activity-1',source_id:'activity-1',source_type:'crm_activity',at:95,href:'/crm/lead-1#crm-activities'},
    {...order,id:'followup:followup-1',source_id:'followup-1',source_type:'followup',at:94,href:'/customer-followups/followup-1'},
    {...order,id:'fulfillment:fulfillment-1',source_id:'fulfillment-1',source_type:'fulfillment',at:93,href:'/commerce/orders/order-1'},
    {...order,id:'return:return-1',source_id:'return-1',source_type:'return',at:92,href:'/commerce/orders/order-1'},
  ];
  const client=createNativeCustomerTimelineClient(
    nativeCustomerTimelineConfiguration(config()),
    vi.fn().mockResolvedValue(ok(page(items))),
  );
  await expect(client.timeline(incoming(),actor,{...input,cursor:undefined,limit:30}))
    .resolves.toMatchObject({items});
 });

 it('rejects a host or request that is not the configured exact authority',async()=>{
  const client=createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config()),vi.fn());
  await expect(client.timeline(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_AUTHORITY_INVALID'});
  for(const invalid of [
    {customerId:'bad customer'},
    {customerId:''},
    {cursor:''},
    {cursor:'x'.repeat(1001)},
    {limit:0},
    {limit:101},
  ]) {
    await expect(client.timeline(incoming(),actor,{...input,...invalid}))
      .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID'});
  }
  await expect(client.timeline(incoming(),{...actor,authority:undefined},input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED'});
  await expect(client.timeline(incoming(),actor,{...input,holdingSlug:'other'}))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED'});
 });

 it('fails closed when the native configuration is absent',async()=>{
  const previous=process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG;
  delete process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG;
  expect(nativeCustomerTimelineAvailable()).toBe(false);
  const {nativeCustomerTimeline}=await import('../../../packages/feature-sales/src/public/native-customer-timeline-page');
  await expect(nativeCustomerTimeline(actor as never,'customer')).rejects.toMatchObject({status:503,code:'NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_REQUIRED'});
  if(previous!==undefined)process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG=previous;
 });

 it('does not fall back when a configured native selector is malformed',async()=>{
  const previous=process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG;
  process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG='{not-json';
  const {nativeCustomerTimeline}=await import('../../../packages/feature-sales/src/public/native-customer-timeline-page');
  await expect(nativeCustomerTimeline(actor as never,'customer')).rejects.toMatchObject({
    code:'NATIVE_CUSTOMER_TIMELINE_CONFIGURATION_INVALID',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG;
  else process.env.ERPX_NATIVE_CUSTOMER_TIMELINE_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config({privateBaseUrl:'http://10.0.0.1:44996'})),vi.fn())).toThrow();
  expect(()=>createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeCustomerTimelineConfiguration('{not-json')).toThrow();
  expect(()=>nativeCustomerTimelineConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativeCustomerTimelineClient(nativeCustomerTimelineConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',async()=>{
  const {nativeCustomerTimelineFailure}=await import('../../../packages/feature-sales/src/public/native-customer-timeline-page');
  expect(nativeCustomerTimelineFailure(new NativeCustomerTimelineError('NATIVE_CUSTOMER_TIMELINE_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeCustomerTimelineFailure(new NativeCustomerTimelineError('NATIVE_CUSTOMER_TIMELINE_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'CUSTOMER_TIMELINE_REQUEST_INVALID'});
  expect(nativeCustomerTimelineFailure(new NativeCustomerTimelineError('NATIVE_CUSTOMER_TIMELINE_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_CUSTOMER_TIMELINE_UNAVAILABLE'});
  expect(nativeCustomerTimelineFailure(new NativeCustomerTimelineError('PACKAGE_REQUIRED',403,'History required.')))
    .toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  expect(nativeCustomerTimelineFailure(new NativeCustomerTimelineError('CUSTOMER_TIMELINE_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'CUSTOMER_TIMELINE_CHANGED'});
  expect(nativeCustomerTimelineFailure(new Error('other'))).toBeUndefined();
 });
});
