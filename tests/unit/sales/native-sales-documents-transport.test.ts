import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeSalesDocumentsClient,
  nativeSalesDocumentsAvailable,
  nativeSalesDocumentTransitionEligible,
  nativeSalesDocumentsConfiguration,
  NativeSalesDocumentsError,
  type NativeSalesDocumentsPageContext,
} from '../../../packages/feature-sales/src/public/native-sales-documents';

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
const actor:NativeSalesDocumentsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['sales'],iam:{permissions:['commerce.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['sales'],permissions:['commerce.read'],
  expiresAt:9_007_199_254_740_991};
const order={id:'order-1',company_id:'company',branch_id:'branch',department_id:null,
  kind:'quote',number:'QT-1',party_id:'customer',party_name:'Customer',state:'draft',
  order_date:'2026-11-04',due_date:'2026-11-18',currency:'THB',subtotal_minor:1000,
  tax_minor:70,total_minor:1070,version:1,created_at:10};
const input={holdingSlug:'acme',kind:'quote' as const,search:'acme',status:'draft',
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const page=(items:unknown[],nextCursor:string|null=null)=>({data:{items,nextCursor,hasMore:nextCursor!==null},context});

describe('native Sales document transport',()=>{
 it('forwards the configured Holding, session, CAS and document query',async()=>{
  const transport=vi.fn().mockResolvedValue(ok(page([order])));
  const client=createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config()),transport);
  const result=await client.documents(incoming(),actor,input);
  expect(result).toEqual({items:[order],nextCursor:null,previousCursor:null,hasMore:false});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/sales/documents?kind=quote&q=acme&status=draft');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug and cursor as context CAS input',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:{items:[{...order,branch_id:'branch'}],nextCursor:'next',hasMore:true},context:branchContext}));
  const client=createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config()),transport);
  await client.documents(incoming(),branchActor,{...input,branchSlug:'north',cursor:'opaque'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/sales/documents?kind=quote&q=acme&status=draft&cursor=opaque');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'SALES_DOCUMENTS_CURSOR_INVALID',message:'Restart this search.'}}),
    {status:422,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config()),transport);
  await expect(client.documents(incoming(),actor,input)).rejects.toMatchObject({
    code:'SALES_DOCUMENTS_CURSOR_INVALID',status:422,message:'Restart this search.',
  });
 });

 it('rejects scope mismatch and malformed page rows before returning data',async()=>{
  const client=(value:unknown)=>createNativeSalesDocumentsClient(
    nativeSalesDocumentsConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  await expect(client(page([{...order,company_id:'foreign'}])).documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
  await expect(client(page([{...order,kind:'sales'}])).documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
  await expect(client(page([{...order,total_minor:1071}])).documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
  await expect(client(page([order,order])).documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
  await expect(client({data:{items:[{...order,branch_id:'other'}],nextCursor:null},context:{...context,branchId:'branch',branchSlug:'north'}}).documents(
    incoming(),{...actor,branchId:'branch'},{...input,branchSlug:'north'},
  )).rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
  await expect(client(page([order],'' )).documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
  await expect(client({data:{items:[order],nextCursor:null,hasMore:false},context:{...context,revision:8}})
    .documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED'});
  await expect(client(page([{...order,extra:'field'}])).documents(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID'});
 });

 it('rejects a host or request that is not the configured exact authority',async()=>{
  const client=createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config()),vi.fn());
  await expect(client.documents(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_AUTHORITY_INVALID'});
  await expect(client.documents(incoming(),actor,{...input,kind:'purchase' as never}))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'});
  await expect(client.documents(incoming(),actor,{...input,search:'x'.repeat(201)}))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'});
  await expect(client.documents(incoming(),actor,{...input,status:'paid'}))
    .rejects.toMatchObject({code:'NATIVE_SALES_DOCUMENTS_REQUEST_INVALID'});
 });

 it('fails closed when the native Sales configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG;
  delete process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG;
  expect(nativeSalesDocumentsAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config({privateBaseUrl:'http://10.0.0.1:44994'})),vi.fn())).toThrow();
  expect(()=>createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeSalesDocumentsConfiguration('{not-json')).toThrow();
  expect(()=>nativeSalesDocumentsConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
});

 it('selects only transitions owned by the existing Commerce public order port',()=>{
  for(const action of ['submit','approve','return','cancel','convert'] as const)
   expect(nativeSalesDocumentTransitionEligible('quote','submitted',action)).toBe(true);
  for(const state of ['draft','submitted','confirmed'])
   for(const action of ['submit','return'] as const)
    expect(nativeSalesDocumentTransitionEligible('sales',state,action)).toBe(true);
  expect(nativeSalesDocumentTransitionEligible('sales','draft','cancel')).toBe(true);
  expect(nativeSalesDocumentTransitionEligible('sales','confirmed','cancel')).toBe(false);
  expect(nativeSalesDocumentTransitionEligible('sales','submitted','approve')).toBe(false);
  expect(nativeSalesDocumentTransitionEligible('sales','confirmed','convert')).toBe(false);
  expect(nativeSalesDocumentTransitionEligible('purchase','draft','submit')).toBe(false);
  expect(nativeSalesDocumentTransitionEligible('quote','draft','delete' as never)).toBe(false);
 });

 it('maps transport failures to the retained page/route status policy',async()=>{
  const {nativeSalesDocumentsFailure}=await import('../../../packages/feature-sales/src/public/native-sales-documents-page');
  expect(nativeSalesDocumentsFailure(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeSalesDocumentsFailure(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'SALES_DOCUMENTS_REQUEST_INVALID'});
  expect(nativeSalesDocumentsFailure(new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_SALES_DOCUMENTS_UNAVAILABLE'});
  expect(nativeSalesDocumentsFailure(new NativeSalesDocumentsError('SALES_DOCUMENTS_SOURCE_INVALID',503,'Unavailable.')))
    .toMatchObject({status:503,code:'NATIVE_SALES_DOCUMENTS_UNAVAILABLE'});
  expect(nativeSalesDocumentsFailure(new NativeSalesDocumentsError('WORK_CONTEXT_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeSalesDocumentsFailure(new Error('other'))).toBeUndefined();
 });
});
