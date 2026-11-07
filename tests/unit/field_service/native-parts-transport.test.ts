import { afterAll, beforeAll, expect, it, vi } from 'vitest';
import { createServer, type Server } from 'node:http';
import { nativeInitialCountRequest } from '@erpx/native-sdk/field-service-initial-count';
import { nativeServiceInvoicePaymentEvidenceRequest,nativeServicePackageRequest,nativeFieldOperationsRequest,nativeFieldCollaborationRequest, nativeFieldSupportRequest, nativePartsPrerequisiteRequest, nativePartsStockRequest, nativePartsSetupRequest } from '@erpx/native-sdk/field-service-parts';

vi.mock('server-only', () => ({}));

let server: Server, baseUrl: string;
const observed: Array<{ method: string; path: string; body: string; context: string | undefined; key?: string; cookie?:string }> = [];
beforeAll(async () => {
  server = createServer(async (request, response) => {
    const chunks: Buffer[] = [];
    for await (const chunk of request) chunks.push(Buffer.from(chunk));
    observed.push({ method: request.method!, path: request.url!, body: Buffer.concat(chunks).toString(), context: request.headers['x-erpx-context-company'] as string | undefined, ...(request.headers['idempotency-key'] ? {key: request.headers['idempotency-key'] as string} : {}),...(request.headers.cookie?{cookie:request.headers.cookie}:{}) });
    response.writeHead(200, { 'content-type': 'application/json' });
    response.end('{"data":{"id":"synthetic-receipt","version":1}}');
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw Error('OWNED_TRANSPORT_LISTENER_REQUIRED');
  baseUrl = `http://127.0.0.1:${address.port}`;
});
afterAll(async () => { await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve())); });

it('carries the original command bytes and captured header to each finite prerequisite path', async () => {
  const headers = new Headers({ 'x-erpx-context-company': 'synthetic-company' });
  const body = '{"reason":" Keep original bytes ","input":{"name":"Synthetic"}}';
  for (const [operation, suffix] of [['read', 'prerequisites'], ['chart', 'charts'], ['account', 'accounts'], ['period', 'periods'], ['rate', 'rates']] as const) {
    const reply = await nativePartsPrerequisiteRequest({ baseUrl, slug: 'acme', operation, headers, timeoutMs: 5_000, ...(operation === 'read' ? {} : { body }) });
    expect(reply).toEqual({ status: 200, body: { data: { id: 'synthetic-receipt', version: 1 } } });
    expect(observed.at(-1)).toEqual({ method: operation === 'read' ? 'GET' : 'POST', path: `/api/v1/holdings/acme/field-service/parts-setup/${suffix}`, body: operation === 'read' ? '' : body, context: 'synthetic-company' });
  }
  await nativePartsSetupRequest({ baseUrl, slug: 'acme', method: 'POST', existing: true, headers, body, timeoutMs: 5_000 });
  expect(observed.at(-1)).toMatchObject({ path: '/api/v1/holdings/acme/field-service/parts-setup/existing', body });
});

it('rejects an unknown prerequisite operation before opening a request', async () => {
  const count = observed.length;
  await expect(nativePartsPrerequisiteRequest({ baseUrl, slug: 'acme', operation: 'toString' as never, headers: new Headers(), timeoutMs: 5_000 })).rejects.toThrow('NATIVE_PARTS_SETUP_REQUEST_INVALID');
  expect(observed).toHaveLength(count);
});

it('uses only the finite Stock routes and preserves original command bytes', async () => {
 const headers=new Headers({'x-erpx-context-company':'synthetic-company'}),body='{ "name" : "Original" }';
 for(const [operation,suffix] of [['read','stock'],['products','masters/products'],['warehouses','masters/warehouses']] as const){
  await nativePartsStockRequest({baseUrl,slug:'acme',operation,headers,timeoutMs:5000,...(operation==='read'?{}:{body})});
  expect(observed.at(-1)).toEqual({method:operation==='read'?'GET':'POST',path:`/api/v1/holdings/acme/field-service/parts-setup/${suffix}`,body:operation==='read'?'':body,context:'synthetic-company'});
 }
 const count=observed.length;await expect(nativePartsStockRequest({baseUrl,slug:'acme',operation:'toString' as never,headers,timeoutMs:5000})).rejects.toThrow('NATIVE_PARTS_SETUP_REQUEST_INVALID');expect(observed).toHaveLength(count);
});

it('Count transport forwards raw selection query and action body to the exact route',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company'}),body='{ "expectedVersion" : 2 }';
 await nativeInitialCountRequest({baseUrl,slug:'acme',route:{kind:'record',id:'count 1',query:'?unknown=one&unknown=two'},headers,timeoutMs:5000});expect(observed.at(-1)).toMatchObject({method:'GET',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count%201?unknown=one&unknown=two'});
 await nativeInitialCountRequest({baseUrl,slug:'acme',route:{kind:'action',id:'count 1',action:'approve'},headers,body,timeoutMs:5000});expect(observed.at(-1)).toMatchObject({method:'POST',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count%201/approve',body,context:'synthetic-company'});
});

it('Count update sends the original body, key and context through the real private HTTP transport',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company','idempotency-key':'original-key'}),body='{ "expectedVersion" : 2, "reason" : "Original" }';
 await nativeInitialCountRequest({baseUrl,slug:'acme',route:{kind:'update',id:'count 1'},headers,body,timeoutMs:5000});
 expect(observed.at(-1)).toEqual({method:'POST',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count%201/update',body,context:'synthetic-company',key:'original-key'});
});
it('generic Count action carries the original body, key and context over private HTTP',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company','idempotency-key':'generic-action-key'}),body='{ "expectedVersion" : 3, "ignored" : {"nested":true} }';
 await nativeInitialCountRequest({baseUrl,slug:'acme',route:{kind:'generic-action',id:'count 1',action:'approve'},headers,body,timeoutMs:5000});
 expect(observed.at(-1)).toEqual({method:'POST',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count%201/stock-count/approve',body,context:'synthetic-company',key:'generic-action-key'});
});
it('generic Count Flow assignment carries original body and key over private HTTP',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company','idempotency-key':'generic-flow-key'}),body='{ "expectedVersion":2, "saveDefault":true }';
 await nativeInitialCountRequest({baseUrl,slug:'acme',route:{kind:'generic-assign',id:'count 1'},headers,body,timeoutMs:5000});
 expect(observed.at(-1)).toEqual({method:'POST',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count%201/stock-count/flow',body,context:'synthetic-company',key:'generic-flow-key'});
});

it('reads the generic Count detail from the finite native route with selected Company context', async () => {
 const headers=new Headers({'x-erpx-context-company':'synthetic-company'});
 await nativeInitialCountRequest({baseUrl,slug:'acme',route:{kind:'generic-detail',id:'count 1'},headers,timeoutMs:5000});
 expect(observed.at(-1)).toMatchObject({method:'GET',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count%201/stock-count',body:'',context:'synthetic-company'});
});

it('Field collaboration preserves original key, context and bytes on its dedicated route',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company','idempotency-key':'original-collaboration'}),body='{ "reason":" original ", "sourceType":"field_service_order" }';
 for(let attempt=0;attempt<2;attempt++){await nativeFieldCollaborationRequest({baseUrl,slug:'acme',method:'POST',headers,body,timeoutMs:5000});expect(observed.at(-1)).toEqual({method:'POST',path:'/api/v1/holdings/acme/field-service/collaboration',body,context:'synthetic-company',key:'original-collaboration'});}
});
it('Support reads encode the exact ticket as one route segment and reject URL dot segments',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company'});
 await nativeFieldSupportRequest({baseUrl,slug:'acme',id:'ticket / source',headers,timeoutMs:5000});
 expect(observed.at(-1)).toEqual({method:'GET',path:'/api/v1/holdings/acme/field-service/support/ticket%20%2F%20source',body:'',context:'synthetic-company'});
 const count=observed.length;
 for(const id of ['', '.', '..', 'x'.repeat(257), 'bad\u0000id'])await expect(nativeFieldSupportRequest({baseUrl,slug:'acme',id,headers,timeoutMs:5000})).rejects.toThrow('NATIVE_FIELD_SUPPORT_REQUEST_INVALID');
 expect(observed).toHaveLength(count);
});

it('Operations carries original bytes to its native root and finite record read path',async()=>{
 const headers=new Headers({'x-erpx-context-company':'synthetic-company','idempotency-key':'ops-original'}),body='{ "entity":"policy", "name":"Original" }';
 await nativeFieldOperationsRequest({baseUrl,slug:'acme',method:'POST',headers,body,timeoutMs:5000});expect(observed.at(-1)).toMatchObject({method:'POST',path:'/api/v1/holdings/acme/field-service/operations',body,key:'ops-original'});
 await nativeFieldOperationsRequest({baseUrl,slug:'acme',method:'GET',headers,id:'case / 1',timeoutMs:5000});expect(observed.at(-1)).toMatchObject({method:'GET',path:'/api/v1/holdings/acme/field-service/operations/case%20%2F%201',body:''});
 const count=observed.length;for(const id of ['.','..','', 'x'.repeat(181)])await expect(nativeFieldOperationsRequest({baseUrl,slug:'acme',method:'GET',headers,id,timeoutMs:5000})).rejects.toThrow('NATIVE_FIELD_OPERATIONS_REQUEST_INVALID');expect(observed).toHaveLength(count);
});

it('uses finite Service Commercial paths and retains exact reviewed payload, credential and idempotency key',async()=>{
 const headers=new Headers({cookie:'__Host-erpx-product=fixture-only','idempotency-key':'captured-service-key'});
 const body='{ "reason" : "'+ 'x'.repeat(70000)+'" }';
 for(const [operation,path]of [['companies','/api/v1/platform/commercial'],['overview','/api/v1/platform/commercial/packages?holdingId=h001&companyId=company'],['preview','/api/v1/platform/commercial/packages/preview'],['apply','/api/v1/platform/commercial/packages/apply']]as const){
  const mutation=operation==='preview'||operation==='apply';
  await nativeServicePackageRequest({baseUrl,slug:'acme',operation,holdingId:'h001',companyId:'company',headers,timeoutMs:5000,...(mutation?{body}:{})});
  expect(observed.at(-1)).toMatchObject({method:mutation?'POST':'GET',path,body:mutation?body:'',key:'captured-service-key',cookie:'__Host-erpx-product=fixture-only'});
 }
 const count=observed.length;
 await expect(nativeServicePackageRequest({baseUrl,slug:'acme',operation:'toString' as never,headers,timeoutMs:5000})).rejects.toThrow('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 await expect(nativeServicePackageRequest({baseUrl,slug:'acme',operation:'apply',headers,timeoutMs:5000,body:' '.repeat(200001)})).rejects.toThrow('NATIVE_PARTS_SETUP_REQUEST_INVALID');
 expect(observed).toHaveLength(count);
});

it('keeps the native invoice supplier-evidence refusal on its exact authenticated read path',async()=>{
 const headers=new Headers({'x-erpx-context-company':'company',cookie:'__Host-erpx-product=fixture-only'});
 await nativeServiceInvoicePaymentEvidenceRequest({baseUrl,slug:'acme',id:'field/invoice',headers,timeoutMs:5000});
 expect(observed.at(-1)).toEqual({method:'GET',path:'/api/v1/holdings/acme/finance/invoices/field%2Finvoice/workspace?purpose=payment-evidence',body:'',context:'company',cookie:'__Host-erpx-product=fixture-only'});
 const count=observed.length;await expect(nativeServiceInvoicePaymentEvidenceRequest({baseUrl,slug:'acme',id:'',headers,timeoutMs:5000})).rejects.toThrow('NATIVE_PARTS_SETUP_REQUEST_INVALID');expect(observed).toHaveLength(count);
});
