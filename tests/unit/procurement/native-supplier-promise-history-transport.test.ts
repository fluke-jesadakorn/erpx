import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeSupplierPromiseHistoryClient,
  nativeProcurementAvailable,
  nativeProcurementConfiguration,
  NativeProcurementError,
  type NativeProcurementPageContext,
} from '../../../packages/feature-procurement/src/public/native-supplier-promise-history';
import { nativeProcurementFailure } from '../../../packages/feature-procurement/src/public/native-supplier-promise-history-page';

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
const actor:NativeProcurementPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['procurement'],iam:{permissions:['commerce.read','module.procurement.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['procurement'],permissions:['commerce.read','module.procurement.read'],
  expiresAt:9_007_199_254_740_991};
const history={id:'promise-2',order_id:'order-1',company_id:'company',order_version:2,
  supplier_id:'supplier',currency:'THB',required_date:'2026-11-10',kind:'confirmed',
  confirmation_date:'2026-11-01',supplier_reference:'SUP-REF-1',
  evidence:'Supplier confirmation evidence',recorded_by:'owner',created_at:20};
const withdrawn={...history,id:'promise-1',order_version:1,kind:'withdrawn',
  confirmation_date:'2026-10-30',supplier_reference:'SUP-REF-0',
  evidence:'Earlier supplier withdrawal'};
const line={id:'promise-line-a1',promise_id:'promise-2',order_line_id:'order-line-a',
  ordinal:0,quantity_milli:4000,fulfilled_snapshot_milli:4000,promised_date:'2026-11-08'};
const open={id:'promise-line-a1',promise_id:'promise-2',order_id:'order-1',company_id:'company',
  branch_id:'branch',department_id:null,order_number:'PO-1',order_state:'confirmed',
  order_line_id:'order-line-a',ordinal:0,quantity_milli:4000,fulfilled_snapshot_milli:4000,
  promised_date:'2026-11-08',product_id:'product-a',description:'Bounded part',
  fulfilled_milli:4000,prior_quantity:0,remaining_milli:4000};
const data={order_id:'order-1',history:[history,withdrawn],lines:[line],open:[open]};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Procurement supplier promise history transport',()=>{
 it('forwards the configured Holding, session, CAS and workspace to the owner read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data,context}));
  const client=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),transport);
  const result=await client.history(incoming(),actor,'order-1',input);
  expect(result).toEqual(data);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/procurement/purchase-orders/order-1/supplier-promises');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug as context CAS input',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data,context:branchContext}));
  const client=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),transport);
  await client.history(incoming(),branchActor,'order-1',{...input,branchSlug:'north'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/procurement/purchase-orders/order-1/supplier-promises');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'SUPPLIER_PROMISE_HISTORY_INVALID',message:'Reload the current supplier history.'}}),
    {status:503,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),transport);
  await expect(client.history(incoming(),actor,'order-1',input)).rejects.toMatchObject({
    code:'SUPPLIER_PROMISE_HISTORY_INVALID',status:503,
    message:'Reload the current supplier history.',
  });
 });

 it('rejects an unconfigured host, changed context and malformed responses before returning data',async()=>{
  const client=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),vi.fn());
  await expect(client.history(incoming({host:'evil.example.com'}),actor,'order-1',input))
    .rejects.toMatchObject({code:'NATIVE_PROCUREMENT_AUTHORITY_INVALID'});
  const changed=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data,context:{...context,revision:8}}),
  ));
  await expect(changed.history(incoming(),actor,'order-1',input))
    .rejects.toMatchObject({code:'NATIVE_PROCUREMENT_CONTEXT_CHANGED'});
  const malformed=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,history:[{...history,extra:'field'}]},context}),
  ));
  await expect(malformed.history(incoming(),actor,'order-1',input))
    .rejects.toMatchObject({code:'NATIVE_PROCUREMENT_RESPONSE_INVALID'});
  const foreign=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,history:[{...history,company_id:'foreign'}]},context}),
  ));
  await expect(foreign.history(incoming(),actor,'order-1',input))
    .rejects.toMatchObject({code:'NATIVE_PROCUREMENT_RESPONSE_INVALID'});
  const missingLine=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,open:[{...open,id:'missing'}]},context}),
  ));
  await expect(missingLine.history(incoming(),actor,'order-1',input))
    .rejects.toMatchObject({code:'NATIVE_PROCUREMENT_RESPONSE_INVALID'});
  const overstated=createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{...data,open:[{...open,remaining_milli:9999}]},context}),
  ));
  await expect(overstated.history(incoming(),actor,'order-1',input))
    .rejects.toMatchObject({code:'NATIVE_PROCUREMENT_RESPONSE_INVALID'});
 });

 it('fails closed when the native Procurement configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_PROCUREMENT_CONFIG;
  delete process.env.ERPX_NATIVE_PROCUREMENT_CONFIG;
  expect(nativeProcurementAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_PROCUREMENT_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config({privateBaseUrl:'http://10.0.0.1:44992'})),vi.fn())).toThrow();
  expect(()=>createNativeSupplierPromiseHistoryClient(nativeProcurementConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeProcurementConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeProcurementFailure(new NativeProcurementError('NATIVE_PROCUREMENT_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeProcurementFailure(new NativeProcurementError('NATIVE_PROCUREMENT_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'PROCUREMENT_REQUEST_INVALID'});
  expect(nativeProcurementFailure(new NativeProcurementError('NATIVE_PROCUREMENT_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_PROCUREMENT_UNAVAILABLE'});
  expect(nativeProcurementFailure(new NativeProcurementError('SUPPLIER_PROMISE_HISTORY_INVALID',503,'Reload current history.')))
    .toMatchObject({status:503,code:'NATIVE_PROCUREMENT_UNAVAILABLE'});
  expect(nativeProcurementFailure(new Error('other'))).toBeUndefined();
 });
});
