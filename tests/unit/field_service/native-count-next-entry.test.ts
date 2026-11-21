import { afterAll, afterEach, beforeAll, beforeEach, expect, it, vi } from 'vitest';
import { createServer, type Server } from 'node:http';

vi.mock('server-only', () => ({}));
const mocks = vi.hoisted(() => ({ authorize: vi.fn(), retained: vi.fn(), incoming: vi.fn() }));
vi.mock('next/headers', () => ({ headers: mocks.incoming }));
vi.mock('@erpx/server-identity_access/public/authorization', () => ({ requireWorkContext: mocks.authorize, publicScope: () => ({ holdingSlug: 'acme' }) }));
vi.mock('@erpx/feature-work_projection/lib/workspace', () => ({ workspaceBase: () => '/holdings/acme' }));
vi.mock('@erpx/server-work_projection/public/page-error', () => ({ pageError: (error: unknown) => { throw error; } }));
vi.mock('@erpx/feature-field_service/components/initial-physical-stock-count', () => ({ ServiceInitialPhysicalStockCount: () => null }));
vi.mock('@erpx/server-inventory/public/service-initial-physical-stock-count', () => ({
  serviceInitialPhysicalStockCountWorkspace: mocks.retained,
  prepareServiceInitialPhysicalStockCount: mocks.retained,
  transitionServiceInitialPhysicalStockCount: mocks.retained,
  serviceInitialPhysicalStockCountFlow: mocks.retained,
  assignServiceInitialPhysicalStockCountFlow: mocks.retained,
  serviceInitialPhysicalStockCountSchema: { parse: mocks.retained },
  serviceInitialPhysicalStockCountActionSchema: { parse: mocks.retained },
  serviceInitialPhysicalStockCountSelectionSchema: { parse: mocks.retained },
  serviceInitialPhysicalStockCountFlowSchema: { parse: mocks.retained },
}));

import { GET, POST } from '../../../packages/feature-service/src/routes/api/v1/holdings/[holdingSlug]/field-service/initial-physical-stock/[[...path]]/route';
import Page from '../../../packages/feature-service/src/routes/holdings/[holdingSlug]/services/field-service/settings/initial-stock/page';
import {serviceCountCommand} from '../../../packages/workplace/src/lib/service-count-native-command';

const nativeContext = { userId:'worker', holdingId:'holding', companyId:'company', branchId:null, departmentId:null, branchSlug:null, departmentSlug:null, assignmentId:'grant', assignmentVersion:3, revision:7, role:'worker', assignmentScopeKind:'holding', modules:['field_service'], permissions:['inventory.read','inventory.prepare'] };
const actor = { ...nativeContext, authority: { assignmentVersion:3, revision:7 }, iam: { permissions:nativeContext.permissions }, scope: { kind:'holding' }, assignmentScope:'holding' };
const view = { companyId:'company', available:true, currency:'THB', documentCurrency:'THB', contextKey:'a'.repeat(64), unavailableReason:null, canPrepare:true, canCreateWarehouse:true, canCreateProduct:true, selection:{warehouseId:'wh',productId:'part',countDate:'2026-01-01'}, readiness:{schemaAvailable:true,postingConfigured:true,varianceAccountAvailable:true,postingPeriodAvailable:true}, branches:[], warehouses:[], products:[], record:null };
const headers = () => new Headers({ host:'acme.example.test', origin:'https://acme.example.test', cookie:'synthetic=opaque', 'x-erpx-context-company':'company', 'x-erpx-context-assignment':'grant', 'x-erpx-context-revision':'7', 'x-erpx-context-assignment-version':'3' });
const params = Promise.resolve({ holdingSlug:'acme' });
let server: Server, baseUrl: string, outage = false;
const observed: Array<{ method:string; path:string; body:string; company:string | undefined; assignment:string | undefined; revision:string | undefined; assignmentVersion:string | undefined; key:string | undefined }> = [];
beforeAll(async () => {
  server = createServer(async (request, response) => {
    const chunks: Buffer[] = [];
    for await (const chunk of request) chunks.push(Buffer.from(chunk));
    observed.push({ method:request.method!, path:request.url!, body:Buffer.concat(chunks).toString(), company:request.headers['x-erpx-context-company'] as string | undefined, assignment:request.headers['x-erpx-context-assignment'] as string | undefined, revision:request.headers['x-erpx-context-revision'] as string | undefined, assignmentVersion:request.headers['x-erpx-context-assignment-version'] as string | undefined, key:request.headers['idempotency-key'] as string | undefined });
    response.setHeader('content-type', 'application/json');
    if (outage) { response.statusCode = 503; response.end(JSON.stringify({error:{code:'OWNER_UNAVAILABLE'}})); return; }
    const update=request.url?.endsWith('/count-a/update');
    if (request.method === 'POST') response.statusCode = update ? 200 : 201;
    response.end(JSON.stringify({ context:{...nativeContext, expiresAt:Date.now()+60_000}, data:request.method === 'POST' ? update ? {id:'count-a',status:'draft',version:4} : {id:'count-original-key',status:'draft',version:1} : view }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string' || [3000,3001,3100,3101,3103,5432,5433,5435,8080].includes(address.port) || address.port >= 3200 && address.port <= 3299) throw Error('OWNED_TRANSPORT_LISTENER_REQUIRED');
  baseUrl = `http://127.0.0.1:${address.port}`;
});
afterAll(async () => { await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve())); });
beforeEach(() => {
  observed.length = 0; outage = false; mocks.authorize.mockReset().mockResolvedValue(actor); mocks.retained.mockReset(); mocks.incoming.mockReset().mockResolvedValue(headers());
  vi.stubEnv('ERPX_NATIVE_FIELD_SERVICE_INITIAL_COUNT_CONFIG', JSON.stringify({privateBaseUrl:baseUrl,timeoutMs:3000,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'holding'}]}));
});
afterEach(() => vi.unstubAllEnvs());

it('actual Count BFF forwards the original selection query, including encoded and duplicate values', async () => {
  const query = '?warehouseId=WH%20%26%20one&productId=%E0%B8%81%2B2&countDate=2026-01-01&productId=other';
  const response = await GET(new Request(`https://acme.example.test/api/v1/holdings/acme/field-service/initial-physical-stock${query}`, {headers:headers()}), {params});
  expect(response.status).toBe(200);
  expect((await response.json()).data).toEqual(view);
  expect(observed).toEqual([{method:'GET',path:`/api/v1/holdings/acme/field-service/initial-physical-stock${query}`,body:'',company:'company',assignment:'grant',revision:'7',assignmentVersion:'3',key:undefined}]);
  expect(mocks.retained).not.toHaveBeenCalled();
});

it('actual Count preparation page sends encoded selection and duplicates through the native SDK', async () => {
  const element = await Page({params,searchParams:Promise.resolve({warehouseId:'WH & one',productId:['ก+2','other'],countDate:'2026-01-01'})});
  expect(observed).toEqual([{method:'GET',path:'/api/v1/holdings/acme/field-service/initial-physical-stock?warehouseId=WH+%26+one&productId=%E0%B8%81%2B2&productId=other&countDate=2026-01-01',body:'',company:'company',assignment:'grant',revision:'7',assignmentVersion:'3',key:undefined}]);
  expect(element.props).toMatchObject({base:'/holdings/acme',initial:view,nativeCount:true,nativeContext});
  expect(mocks.retained).not.toHaveBeenCalled();
});

it('actual Count BFF preserves the captured command bytes and idempotency key', async () => {
  const incoming = headers(); incoming.set('idempotency-key', 'original-key'); incoming.set('content-type', 'application/json');
  const body = '{ "expectedVersion": 0, "contextKey": "original" }';
  const response = await POST(new Request('https://acme.example.test/api/v1/holdings/acme/field-service/initial-physical-stock', {method:'POST',headers:incoming,body}), {params});
  expect(response.status).toBe(201);
  expect((await response.json()).data).toEqual({id:'count-original-key',status:'draft',version:1});
  expect(observed).toEqual([{method:'POST',path:'/api/v1/holdings/acme/field-service/initial-physical-stock',body,company:'company',assignment:'grant',revision:'7',assignmentVersion:'3',key:'original-key'}]);
  expect(mocks.retained).not.toHaveBeenCalled();
});

it('selected native failures do not fall back to the retained Count owner', async () => {
  outage = true;
  const response = await GET(new Request('https://acme.example.test/api/v1/holdings/acme/field-service/initial-physical-stock?warehouseId=wh', {headers:headers()}), {params});
  expect(response.status).toBe(503);
  expect((await response.json()).error.code).toBe('OWNER_UNAVAILABLE');
  await expect(Page({params,searchParams:Promise.resolve({warehouseId:'wh'})})).rejects.toMatchObject({status:503,code:'OWNER_UNAVAILABLE'});
  expect(observed).toHaveLength(2);
  expect(mocks.retained).not.toHaveBeenCalled();
});

it('generic Service native helper forwards raw update after captured context checks',async()=>{
 const incoming=headers(),raw='{ "expectedVersion":3, "unknown":{"x":1} }',request=new Request('https://acme.example.test/api/v1/holdings/acme/stock-counts/count-a',{method:'POST',headers:incoming,body:raw});
 const result=await serviceCountCommand(request,actor as never,'acme',{kind:'update',id:'count-a'},'original-key');expect(result.data).toEqual({id:'count-a',status:'draft',version:4});expect(result.raw).toBe(raw);expect(observed[0]).toMatchObject({method:'POST',path:'/api/v1/holdings/acme/field-service/initial-physical-stock/count-a/update',body:raw,key:'original-key',company:'company',assignment:'grant',revision:'7',assignmentVersion:'3'});
});
it('generic Service native helper rejects missing config or stale context before reading body',async()=>{
 const raw='{ "expectedVersion":3 }',missing=headers();missing.delete('x-erpx-context-assignment-version');const stale=new Request('https://acme.example.test/api',{method:'POST',headers:missing,body:raw});await expect(serviceCountCommand(stale,actor as never,'acme',{kind:'update',id:'count-a'},'key')).rejects.toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});expect(stale.bodyUsed).toBe(false);
 vi.stubEnv('ERPX_NATIVE_FIELD_SERVICE_INITIAL_COUNT_CONFIG',undefined);const request=new Request('https://acme.example.test/api',{method:'POST',headers:headers(),body:raw});await expect(serviceCountCommand(request,actor as never,'acme',{kind:'update',id:'count-a'},'key')).rejects.toMatchObject({status:503,code:'NATIVE_INITIAL_COUNT_UNAVAILABLE'});expect(request.bodyUsed).toBe(false);expect(observed).toHaveLength(0);
});
