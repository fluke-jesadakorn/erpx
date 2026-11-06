import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createNativeQualityReader} from '../../../packages/feature-quality/src/adapters/server/native-quality';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44991',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
  'x-erpx-context-company':'company',
  'x-erpx-context-assignment':'quality-assignment',
  'x-erpx-context-revision':'7',
  'x-erpx-context-assignment-version':'3',
  ...overrides,
});
const decision={canInspect:false,canRelease:true,canCancel:false,canReviewCancellation:false,canDisposition:false};
const hold={
  id:'hold-a',version:2,status:'awaiting_release',lastResult:'pass',lastInspector:'inspector',
  createdBy:'maker',dueDate:'2026-11-10',reason:'Damaged packaging requires independent review',
  warehouseId:'warehouse',productId:'product',lotId:null,heldQuantityMilli:null,inspectionCount:1,
  decision,nextAction:'Approve release of the passing inspection',
};
const workspace={holds:[hold],canHold:true,canManageSpecs:true};
const detail={
  hold,inspections:[{id:'inspection-1',holdId:'hold-a',reviewedVersion:1,result:'pass',
    evidence:'Independent inspection evidence',inspectedBy:'inspector',createdAt:1,specId:null,specVersion:null}],
  measurements:[{id:'measurement-1',inspectionId:'inspection-1',metricId:'metric-weight',
    code:'weight',label:'Weight',unit:'kg',minimum:'9.9',maximum:'10.1',measuredValue:'10.0',
    passed:true,ordinal:0}],
  cancellations:[{id:'cancel-1',holdId:'hold-a',version:1,action:'request',
    reason:'Mistaken scope',actorId:'maker',createdAt:0}],
  dispositions:[],specMetrics:[],alerts:[],
};
const receipt={id:'hold-a',holdId:'hold-a',cancellationId:'quality-cancel-http',status:'cancel_pending',version:2};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Quality transport',()=>{
 it('forwards the configured Holding and current session to the workspace read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace}));
  const client=createNativeQualityReader(config(),transport);
  const result=await client.workspace(incoming(),'acme');
  expect(result.holds[0]).toMatchObject({id:'hold-a',status:'awaiting_release',decision:{canRelease:true}});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/quality');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('quality-assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(body).toBeUndefined();
 });

 it('reads detail by id and exposes the native decision and inspection history',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:detail}));
  const client=createNativeQualityReader(config(),transport);
  const result=await client.detail(incoming({'x-erpx-workspace-branch':'bangkok'}),'acme','hold-a');
  expect(result.hold.decision.canRelease).toBe(true);
  expect(result.inspections[0]).toMatchObject({id:'inspection-1',result:'pass'});
  expect(result.cancellations[0]).toMatchObject({action:'request',version:1});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/quality/hold-a');
  expect(headers.get('x-erpx-workspace-branch')).toBe('bangkok');
 });

 it('sends one keyed cancellation with origin and returns the owner acknowledgement',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:receipt}));
  const client=createNativeQualityReader(config(),transport);
  const result=await client.cancel(incoming({'x-erpx-workspace-branch':'bangkok','x-erpx-workspace-department':'inspection'}),'acme','hold-a',{
    expectedVersion:1,action:'request',reason:'Incorrect receiving hold',
  },'quality-cancel-http');
  expect(result).toEqual(receipt);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/quality/hold-a/cancel');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('quality-assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('x-erpx-workspace-branch')).toBe('bangkok');
  expect(headers.get('x-erpx-workspace-department')).toBe('inspection');
  expect(headers.get('idempotency-key')).toBe('quality-cancel-http');
  expect(headers.get('content-type')).toBe('application/json');
  expect(JSON.parse(body)).toEqual({expectedVersion:1,action:'request',reason:'Incorrect receiving hold'});
 });

 it('forwards missing or mismatched selected context to the owner without inventing authority',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'WORK_CONTEXT_CHANGED',message:'Reload and review.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeQualityReader(config(),transport);
  const selected=incoming({'x-erpx-context-company':'other-company'});
  selected.delete('x-erpx-context-assignment');
  selected.delete('x-erpx-context-assignment-version');
  await expect(client.cancel(selected,'acme','hold-a',{
    expectedVersion:1,action:'request',reason:'Incorrect receiving hold',
  },'quality-cancel-http')).rejects.toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  const [, , headers, body]=transport.mock.calls[0];
  expect(headers.get('x-erpx-context-company')).toBe('other-company');
  expect(headers.get('x-erpx-context-assignment')).toBeNull();
  expect(headers.get('x-erpx-context-assignment-version')).toBeNull();
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('idempotency-key')).toBe('quality-cancel-http');
  expect(JSON.parse(body)).toEqual({expectedVersion:1,action:'request',reason:'Incorrect receiving hold'});
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'QUALITY_CANCELLATION_CONFLICT',message:'The saved Quality cancellation differs from this request.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeQualityReader(config(),transport);
  await expect(client.cancel(incoming(),'acme','hold-a',{
    expectedVersion:1,action:'approve',reason:'Independent review',
  },'quality-cancel-http')).rejects.toMatchObject({
    code:'QUALITY_CANCELLATION_CONFLICT',status:409,
    message:'The saved Quality cancellation differs from this request.',
  });
 });

 it('rejects an unknown host or slug before transport and refuses malformed responses',async()=>{
  const client=createNativeQualityReader(config(),vi.fn());
  await expect(client.workspace(incoming({host:'evil.example.com'}),'acme'))
    .rejects.toMatchObject({code:'NATIVE_QUALITY_AUTHORITY_INVALID'});
  await expect(client.workspace(incoming(),'other'))
    .rejects.toMatchObject({code:'NATIVE_QUALITY_AUTHORITY_INVALID'});
  const malformed=createNativeQualityReader(config(),vi.fn().mockResolvedValue(
    ok({data:{...workspace,holds:[{...hold,decision:{...decision,canRelease:'yes'}}]}}),
  ));
  await expect(malformed.workspace(incoming(),'acme'))
    .rejects.toMatchObject({code:'NATIVE_QUALITY_RESPONSE_INVALID'});
 });

 it('rejects reserved loopback ports, insecure origins and a missing command key',async()=>{
  expect(()=>createNativeQualityReader(config({privateBaseUrl:'http://127.0.0.1:3100'}),vi.fn())).toThrow();
  expect(()=>createNativeQualityReader(config({privateBaseUrl:'http://10.0.0.1:44991'}),vi.fn())).toThrow();
  expect(()=>createNativeQualityReader(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme'}],
  }),vi.fn())).toThrow();
  const transport=vi.fn();
  const client=createNativeQualityReader(config(),transport);
  await expect(client.cancel(incoming(),'acme','hold-a',{},
    '')).rejects.toMatchObject({code:'COMMAND_KEY_REQUIRED'});
  expect(transport).not.toHaveBeenCalled();
 });
});
