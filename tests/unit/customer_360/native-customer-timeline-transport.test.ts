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

 it('shares one canonical customer-visibility predicate across the directory and visible-customer reads',async()=>{
  const {customerVisiblePredicate}=await import('../../../packages/server-customer_360/src/adapters/customer-read-sources');
  const expected="(EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id) OR EXISTS(SELECT 1 FROM scoped_service g WHERE g.customer_id=p.id) OR EXISTS(SELECT 1 FROM scoped_invoices i WHERE i.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_legal lg WHERE lg.order_id IN (SELECT id FROM scoped_orders so WHERE so.party_id=p.id)))";
  expect(customerVisiblePredicate(true,true,true)).toBe(expected);
  expect(customerVisiblePredicate(false,false,false)).toBe("(EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id) OR EXISTS(SELECT 1 FROM scoped_leads l WHERE l.customer_id=p.id) OR FALSE OR FALSE OR FALSE)");
  expect(customerVisiblePredicate(true,true,true).match(/SELECT /g)).toHaveLength(6);
  const source=await (await import('node:fs/promises')).readFile(new URL('../../../packages/server-customer_360/src/adapters/timeline.ts',import.meta.url),'utf8');
  expect(source.match(/customerVisiblePredicate\(serviceVisible, financeVisible, legalVisible\)/g)).toHaveLength(2);
  // r10: the customer-party projection now composes through the shared scopedProjection helper.
  expect(source).not.toContain('SELECT p.id,p.name,p.code');
  expect(source.match(/scopedProjection\('p\.id,p\.name,p\.code'/g)).toHaveLength(2);
 });

 it('shares one parameterized projection across the non-alias timeline event family',async()=>{
  const read=async(path:string)=> (await import('node:fs/promises')).readFile(new URL(path,import.meta.url),'utf8');
  const timeline=await read('../../../packages/server-customer_360/src/adapters/timeline.ts');
  const scoped=await read('../../../packages/server-customer_360/src/adapters/customer-read-sources.ts');
  // r6 eventSelect and r5 scopedRow stay authoritative and untouched.
  expect(timeline.match(/const eventSelect=\(/g)).toHaveLength(1);
  expect(scoped.match(/const scopedRow = \(/g)).toHaveLength(1);
  // The five remaining event rows share one helper; no literal SELECT remains for them.
  expect(timeline.match(/const eventRow=\(/g)).toHaveLength(1);
  expect(timeline.match(/\$\{eventRow\(/g)).toHaveLength(5);
  for(const prefix of ['fulfillment:','return:','contact:','invoice:','crm-activity:'])
   expect(timeline).not.toContain(`SELECT '${prefix}'`);
 });

 it('shares one byte-identical company scope source across package-obligation facts and the portal projection',async()=>{
  const {companyScopeSource}=await import('../../../packages/server-customer_360/src/adapters/customer-read-sources');
  expect(companyScopeSource('')).toBe('(SELECT ?::text company_id)');
  expect(companyScopeSource('portal_release')).toBe('(SELECT ?::text company_id) portal_release');
  const read=async(path:string)=> (await import('node:fs/promises')).readFile(new URL(path,import.meta.url),'utf8');
  const scoped=await read('../../../packages/server-customer_360/src/adapters/customer-read-sources.ts');
  const facts=await read('../../../packages/server-customer_360/src/adapters/package-obligations.ts');
  const portal=await read('../../../packages/server-customer_360/src/adapters/portal.ts');
  // One source definition; both call sites render through it.
  expect(scoped.match(/companyScopeSource = \(/g)).toHaveLength(1);
  expect(facts).toContain("WITH scope AS${companyScopeSource('')}");
  expect(portal).toContain("FROM ${companyScopeSource('portal_release')}");
  expect(facts).not.toContain('WITH scope AS(SELECT ?::text company_id)');
  expect(portal).not.toContain('FROM (SELECT ?::text company_id) portal_release');
  // Locks and the r5/r6/r7 helpers stay untouched.
  expect(facts).toContain('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE');
  expect(scoped.match(/const scopedRow = \(/g)).toHaveLength(1);
  const timeline=await read('../../../packages/server-customer_360/src/adapters/timeline.ts');
  expect(timeline.match(/const eventSelect=\(/g)).toHaveLength(1);
  expect(timeline.match(/const eventRow=\(/g)).toHaveLength(1);
 });

 it('shares one parameterized company/scope projection across the scoped CTE family (r9)',async()=>{
 const read=async(path:string)=> (await import('node:fs/promises')).readFile(new URL(path,import.meta.url),'utf8');
 const timeline=await read('../../../packages/server-customer_360/src/adapters/timeline.ts');
 expect(timeline).not.toContain('const scopedProjection = ('); // r13 single head now in customer-read-sources
 expect(timeline).toContain('scopedProjection } from "./customer-read-sources"');
 expect(timeline.match(/\$\{scopedProjection\(/g)).toHaveLength(8); // r9 six scoped CTE reads + r10 two directory/visible-customer reads
 for(const literal of [
  'WITH scoped_orders AS (SELECT id,company_id,party_id',
  'scoped_leads AS (SELECT l.id,l.title',
  'scoped_service AS (SELECT g.id,g.customer_id',
  'scoped_invoices AS (SELECT i.id,i.party_id',
  'scoped_legal AS (SELECT i.id,i.title',
  'customers AS (SELECT id,name',
 ]) expect(timeline).not.toContain(literal);
 // r5-r8 helpers, the visibility predicate and the package-obligation lock stay authoritative.
 const scoped=await read('../../../packages/server-customer_360/src/adapters/customer-read-sources.ts');
 expect(scoped.match(/const scopedRow = \(/g)).toHaveLength(1);
 expect(scoped.match(/companyScopeSource = \(/g)).toHaveLength(1);
 expect(timeline.match(/const eventSelect=\(/g)).toHaveLength(1);
 expect(timeline.match(/const eventRow=\(/g)).toHaveLength(1);
 expect(timeline).not.toContain('SELECT p.id,p.name,p.code'); // r10 customer-party projection now composes through scopedProjection
 const facts=await read('../../../packages/server-customer_360/src/adapters/package-obligations.ts');
 expect(facts).toContain('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE');
 const portal=await read('../../../packages/server-customer_360/src/adapters/portal.ts');
 expect(portal).toContain("companyScopeSource('portal_release')");
 });

  it('shares one parameterized customer-party projection with the scoped CTE family (r10)',async()=>{
   const read=async(path:string)=> (await import('node:fs/promises')).readFile(new URL(path,import.meta.url),'utf8');
   const timeline=await read('../../../packages/server-customer_360/src/adapters/timeline.ts');
   // Mirror of the single production scopedProjection template; byte-identity oracle only, never a live source.
   const scopedProjection=(columns:string,from:string,where:string)=>`SELECT ${columns} FROM ${from} WHERE ${where}`;
   const parties='(PARTIES) p',pred="EXISTS(SELECT 1 FROM scoped_orders o WHERE o.party_id=p.id)";
   // Verbatim pre-dedupe renderings of the deleted customerPartySelect helper.
   const directory=`SELECT p.id,p.name,p.code FROM ${parties} WHERE p.company_id=? AND p.kind='customer' AND ${pred}`;
   const visible=`SELECT p.id,p.name,p.code FROM ${parties} WHERE p.id=? AND p.company_id=? AND p.kind='customer' AND ${pred}`;
   expect(scopedProjection('p.id,p.name,p.code',parties,`p.company_id=? AND p.kind='customer' AND ${pred}`)).toBe(directory);
   expect(scopedProjection('p.id,p.name,p.code',parties,`p.id=? AND p.company_id=? AND p.kind='customer' AND ${pred}`)).toBe(visible);
   expect(directory).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
   expect(visible).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|LOCK\s+TABLE/i);
   // One definition; eight composed sites (six r9 + two r10); the pre-r5 helper is gone.
   expect(timeline).not.toContain('const scopedProjection = ('); // r13: head moved to customer-read-sources
   expect(timeline).toContain('scopedProjection } from "./customer-read-sources"');
   expect(timeline.match(/\$\{scopedProjection\(/g)).toHaveLength(8);
   expect(timeline).not.toContain('customerPartySelect');
   expect(timeline).not.toContain('SELECT p.id,p.name,p.code');
   // r5-r9 helpers, the visibility predicate and every lock stay authoritative and untouched.
   expect(timeline.match(/const eventSelect=\(/g)).toHaveLength(1);
   expect(timeline.match(/const eventRow=\(/g)).toHaveLength(1);
   const scoped=await read('../../../packages/server-customer_360/src/adapters/customer-read-sources.ts');
   expect(scoped.match(/const scopedRow = \(/g)).toHaveLength(1);
   expect(scoped.match(/companyScopeSource = \(/g)).toHaveLength(1);
   const facts=await read('../../../packages/server-customer_360/src/adapters/package-obligations.ts');
   expect(facts).toContain('LOCK TABLE customer_portal_actions IN SHARE ROW EXCLUSIVE MODE');
   expect(facts).toContain("WITH scope AS${companyScopeSource('')}");
   const portal=await read('../../../packages/server-customer_360/src/adapters/portal.ts');
   expect(portal).toContain("companyScopeSource('portal_release')");
   expect(portal).toContain('SELECT id,binding_id,order_id,action,status,created_at,request_hash,user_id FROM customer_portal_actions WHERE');
   expect(timeline).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
  });

  it('r14 scoped-projection head family: retirement:ts-statements is strictly below BEFORE=12',async()=>{
   const fs=await import('node:fs/promises');
   const {fileURLToPath}=await import('node:url');
   const {resolve}=await import('node:path');
   const root=fileURLToPath(new URL('../../../packages/server-customer_360/src',import.meta.url));
   let total=0;
   const walk=async(dir:string):Promise<void>=>{
    for(const name of await fs.readdir(dir)){
     const p=resolve(dir,name);
     if((await fs.stat(p)).isDirectory()) await walk(p);
     else if(p.endsWith('.ts')) total+=((await fs.readFile(p,'utf8')).match(/SELECT/g)??[]).length;
    }
   };
   await walk(root);
   expect(total).toBe(11);
   expect(total).toBeLessThan(12);
  });
});
