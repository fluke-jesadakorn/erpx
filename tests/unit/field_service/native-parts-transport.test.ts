import { afterAll, beforeAll, expect, it, vi } from 'vitest';
import { createServer, type Server } from 'node:http';
import { nativeInitialCountRequest } from '@erpx/native-sdk/field-service-initial-count';
import { nativePartsPrerequisiteRequest, nativePartsStockRequest, nativePartsSetupRequest } from '@erpx/native-sdk/field-service-parts';

vi.mock('server-only', () => ({}));

let server: Server, baseUrl: string;
const observed: Array<{ method: string; path: string; body: string; context: string | undefined; key?: string }> = [];
beforeAll(async () => {
  server = createServer(async (request, response) => {
    const chunks: Buffer[] = [];
    for await (const chunk of request) chunks.push(Buffer.from(chunk));
    observed.push({ method: request.method!, path: request.url!, body: Buffer.concat(chunks).toString(), context: request.headers['x-erpx-context-company'] as string | undefined, ...(request.headers['idempotency-key'] ? {key: request.headers['idempotency-key'] as string} : {}) });
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
