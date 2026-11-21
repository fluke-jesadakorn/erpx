import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeEcommerceCatalogClient,
  nativeEcommerceCatalogAvailable,
  nativeEcommerceCatalogConfiguration,
  NativeEcommerceCatalogError,
  type NativeEcommerceCatalogPageContext,
} from '../../../packages/feature-sales/src/public/native-ecommerce-catalog';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44994',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeEcommerceCatalogPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['ecommerce'],iam:{permissions:['commerce.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['ecommerce'],permissions:['commerce.read'],
  expiresAt:9_007_199_254_740_991};
const channel={id:'channel-1',company_id:'company',name:'Storefront',channel_type:'web',
  configuration_json:'{"storefront":"https://shop.example"}',status:'active',
  created_by:'creator',reviewed_by:'reviewer',version:2,created_at:1,updated_at:2};
const listing={id:'listing-1',channel_id:'channel-1',product_id:'product-1',
  title:'Online product',currency:'THB',price_minor:12500,status:'published',
  created_by:'creator',reviewed_by:'reviewer',version:2,created_at:1,updated_at:2};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',
  revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const page=(channels:unknown[],listings:unknown[])=>({data:{channels,listings},context});

describe('native Ecommerce catalog transport',()=>{
 it('forwards the configured Holding, session and context CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok(page([channel],[listing])));
  const client=createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config()),transport);
  const result=await client.catalog(incoming(),actor,input);
  expect(result).toEqual({channels:[channel],listings:[listing]});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/ecommerce/catalog');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace context',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:{channels:[{...channel,company_id:'company'}],listings:[]},context:branchContext}));
  const client=createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config()),transport);
  await client.catalog(incoming(),branchActor,{...input,branchSlug:'north'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/ecommerce/catalog');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'ECOMMERCE_CATALOG_TOO_LARGE',message:'Narrow this catalog.'}}),
    {status:503,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config()),transport);
  await expect(client.catalog(incoming(),actor,input)).rejects.toMatchObject({
    code:'ECOMMERCE_CATALOG_TOO_LARGE',status:503,message:'Narrow this catalog.',
  });
 });

 it('rejects foreign, duplicate, orphan, unordered and malformed rows',async()=>{
  const client=(value:unknown)=>createNativeEcommerceCatalogClient(
    nativeEcommerceCatalogConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  for(const value of [
    page([{...channel,company_id:'foreign'}],[listing]),
    page([channel,channel],[listing]),
    page([channel],[{...listing,channel_id:'other'}]),
    page([channel],[{...listing,status:'active'}]),
    page([channel],[{...listing,currency:'thb'}]),
    page([channel],[{...listing,price_minor:-1}]),
    page([channel],[{...listing,version:0}]),
    page([channel],[{...listing,extra:'field'}]),
    page([{...channel,configuration_json:'[]'}],[listing]),
    page([{...channel,created_at:3,updated_at:2}],[listing]),
    page([channel],[listing,{...listing,id:'listing-2'}]),
    page([{...channel,updated_at:1},channel],[listing]),
  ]) {
    await expect(client(value).catalog(incoming(),actor,input))
      .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_RESPONSE_INVALID'});
  }
  await expect(client({data:{channels:[channel],listings:[listing]},context:{...context,revision:8}})
    .catalog(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED'});
  await expect(client({data:{channels:[channel],listings:[listing]},context:{...context,companyId:'other'}})
    .catalog(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED'});
 });

 it('rejects a host or request that is not the configured exact authority',async()=>{
  const client=createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config()),vi.fn());
  await expect(client.catalog(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_AUTHORITY_INVALID'});
  await expect(client.catalog(incoming(),{...actor,holdingSlug:'other'},input))
    .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED'});
  await expect(client.catalog(incoming(),actor,{...input,holdingSlug:'other'}))
    .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED'});
  await expect(client.catalog(incoming(),{...actor,authority:undefined},input))
    .rejects.toMatchObject({code:'NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED'});
 });

 it('fails closed when the native Ecommerce configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG;
  delete process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG;
  expect(nativeEcommerceCatalogAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_ECOMMERCE_CATALOG_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config({privateBaseUrl:'http://10.0.0.1:44994'})),vi.fn())).toThrow();
  expect(()=>createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config({
    holdings:[{origin:'https://acme.example.com',holdingSlug:'ACME',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeEcommerceCatalogConfiguration('{not-json')).toThrow();
  expect(()=>nativeEcommerceCatalogConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativeEcommerceCatalogClient(nativeEcommerceCatalogConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',async()=>{
  const {nativeEcommerceCatalogFailure}=await import('../../../packages/feature-sales/src/public/native-ecommerce-catalog-page');
  expect(nativeEcommerceCatalogFailure(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeEcommerceCatalogFailure(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_REQUEST_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE'});
  expect(nativeEcommerceCatalogFailure(new NativeEcommerceCatalogError('NATIVE_ECOMMERCE_CATALOG_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE'});
  expect(nativeEcommerceCatalogFailure(new NativeEcommerceCatalogError('ECOMMERCE_CATALOG_TOO_LARGE',503,'Unavailable.')))
    .toMatchObject({status:503,code:'NATIVE_ECOMMERCE_CATALOG_UNAVAILABLE'});
  expect(nativeEcommerceCatalogFailure(new NativeEcommerceCatalogError('ECOMMERCE_SOURCE_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'ECOMMERCE_SOURCE_CHANGED'});
  expect(nativeEcommerceCatalogFailure(new Error('other'))).toBeUndefined();
 });
});
