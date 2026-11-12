import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {createNativeQualityReader} from '../../../packages/feature-quality/src/adapters/server/native-quality';
import {qualityHistorySessionKey,mergeQualityHistory,beginQualityHistoryRequest,finishQualityHistoryRequest} from '../../../packages/feature-quality/src/lib/quality-cancellation-history';
import {QualityCancellationHistory} from '../../../packages/feature-quality/src/components/quality-cancellation-history';

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
  id:'hold-a',companyId:'company',branchId:'branch',warehouseId:'warehouse',productId:'product',lotId:'lot-1',
  reason:'Damaged packaging requires independent review',dueDate:'2026-11-10',status:'awaiting_release',
  createdBy:'maker',lastInspector:'inspector',lastResult:'pass',releasedBy:null,releaseReason:null,releasedAt:null,
  version:2,createdAt:1,updatedAt:2,specId:'spec-1',specVersion:2,quantityScope:'partial',heldQuantityMilli:1000,
  sourceType:'goods_receipt',sourceId:'receipt-1',cancelRequestedBy:null,cancelReason:null,cancelledBy:null,
  cancelledAt:null,dispositionKind:null,dispositionReason:null,dispositionRequestedBy:null,dispositionCountId:null,
  dispositionQuantityMilli:null,disposedBy:null,disposedAt:null,inspectionCount:1,productName:'Packaged goods',
  productUnit:'carton',trackingMode:'lot',warehouseName:'Main warehouse',lotCode:'LOT-1',serialNumber:null,
  expiryDate:'2027-01-01',eligibleCount:2,decision,nextAction:'Approve release of the passing inspection',
};
const stock={warehouseId:'warehouse',productId:'product',quantityMilli:2000,version:4,
  productName:'Packaged goods',trackingMode:'lot',warehouseName:'Main warehouse'};
const lot={id:'lot-1',warehouseId:'warehouse',productId:'product',quantityMilli:1000,balanceVersion:3,
  productName:'Packaged goods',warehouseName:'Main warehouse',trackingMode:'lot',lotCode:'LOT-1',
  serialNumber:null,expiryDate:'2027-01-01',qualityHeld:false};
const product={id:'product',sku:'SKU-1',name:'Packaged goods'};
const specification={id:'spec-1',productId:'product',name:'Receiving limits',status:'active',version:2,
  sku:'SKU-1',productName:'Packaged goods'};
const metric={id:'metric-weight',specId:'spec-1',code:'weight',label:'Weight',unit:'kg',minimum:'9.9',
  maximum:'10.1',ordinal:0};
const workspace={holds:[hold],stock:[stock],lots:[lot],products:[product],specifications:[specification],
  specMetrics:[metric],canHold:true,canManageSpecs:true};
const detail={
  hold,inspections:[{id:'inspection-1',holdId:'hold-a',reviewedVersion:1,result:'pass',
    evidence:'Independent inspection evidence',inspectedBy:'inspector',createdAt:1,specId:null,specVersion:null}],
  measurements:[{id:'measurement-1',inspectionId:'inspection-1',metricId:'metric-weight',
    code:'weight',label:'Weight',unit:'kg',minimum:'9.9',maximum:'10.1',measuredValue:'10.0',
    passed:true,ordinal:0}],
  cancellations:[{id:'cancel-1',holdId:'hold-a',version:1,action:'request',
    reason:'Mistaken scope',actorId:'maker',createdAt:0}],
  dispositions:[],specMetrics:[metric],alerts:[],
};
const receipt={id:'hold-a',holdId:'hold-a',cancellationId:'quality-cancel-http',status:'cancel_pending',version:2};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Quality transport',()=>{
 it('resets sessions by selected context and never treats an empty continuation as completion',()=>{
  expect(qualityHistorySessionKey('hold-a','company-a:assignment-a:1')).not.toBe(qualityHistorySessionKey('hold-a','company-b:assignment-b:1'));
  expect(qualityHistorySessionKey('hold-a','company-a:assignment-a:1')).not.toBe(qualityHistorySessionKey('hold-a','company-a:assignment-a:2'));
  const revision=`qch1:${'a'.repeat(64)}`;
  const item={id:'cancel-1',holdId:'hold-a',version:7,action:'request' as const,reason:'Incorrect scope',actorId:'maker',createdAt:1};
  const current={items:[item],nextBefore:7,headVersion:9,historyRevision:revision};
  const first=QualityCancellationHistory({holdId:'hold-a',endpoint:'/quality/hold-a/cancellations',initialPage:current,contextResetKey:'company-a:assignment-a:1'});
  const switched=QualityCancellationHistory({holdId:'hold-a',endpoint:'/quality/hold-a/cancellations',initialPage:current,contextResetKey:'company-b:assignment-b:1'});
  expect(first.key).not.toBe(switched.key);
  const samePage=QualityCancellationHistory({holdId:'hold-a',endpoint:'/quality/hold-a/cancellations',initialPage:{...current,items:[...current.items]},contextResetKey:'company-a:assignment-a:1'});
  expect(first.key).toBe(samePage.key);
  const changedPage=QualityCancellationHistory({holdId:'hold-a',endpoint:'/quality/hold-a/cancellations',initialPage:{...current,historyRevision:`qch1:${'b'.repeat(64)}`},contextResetKey:'company-a:assignment-a:1'});
  expect(first.key).not.toBe(changedPage.key);
  const slot:{current:AbortController|null}={current:null}, a=new AbortController(), b=new AbortController();
  expect(beginQualityHistoryRequest(slot,a)).toBe(true);
  expect(beginQualityHistoryRequest(slot,b)).toBe(false);
  finishQualityHistoryRequest(slot,b);expect(slot.current).toBe(a);
  finishQualityHistoryRequest(slot,a);expect(beginQualityHistoryRequest(slot,b)).toBe(true);
  expect(mergeQualityHistory(current,{...current,items:[],nextBefore:null})).toBeNull();
  expect(mergeQualityHistory(current,{...current,items:[{...item,id:'cancel-2',version:5}],nextBefore:null,historyRevision:`qch1:${'b'.repeat(64)}`})).toBeNull();
  expect(mergeQualityHistory(current,{...current,items:[{...item,version:5}],nextBefore:null})).toBeNull();
  expect(mergeQualityHistory(current,{...current,items:[{...item,id:'cancel-2',version:5}],nextBefore:null})?.items.map(x=>x.version)).toEqual([7,5]);
 });
 it('preserves an authenticated history cursor and rejects inconsistent pages',async()=>{
  const revision=`qch1:${'a'.repeat(64)}`;
  const item={id:'cancel-1',holdId:'hold-a',version:7,action:'request',reason:'Incorrect scope',actorId:'maker',createdAt:1};
  const transport=vi.fn().mockResolvedValue(ok({data:{items:[item],nextBefore:7,headVersion:9,historyRevision:revision}}));
  const client=createNativeQualityReader(config(),transport);
  await expect(client.cancellationHistory(incoming(),'acme','hold-a')).resolves.toMatchObject({items:[item],nextBefore:7,headVersion:9,historyRevision:revision});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');expect(url.pathname).toBe('/api/v1/holdings/acme/quality/hold-a/cancellations');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');expect(headers.get('x-erpx-context-company')).toBe('company');
  transport.mockResolvedValueOnce(ok({data:{items:[{...item,id:'cancel-2',version:5}],nextBefore:null,headVersion:9,historyRevision:revision}}));
  await client.cancellationHistory(incoming(),'acme','hold-a',{before:7,head:9,revision});
  expect(transport.mock.calls[1][1].searchParams.get('before')).toBe('7');
  expect(transport.mock.calls[1][1].searchParams.get('revision')).toBe(revision);
  transport.mockResolvedValueOnce(ok({data:{items:[item],nextBefore:null,headVersion:10,historyRevision:revision}}));
  await expect(client.cancellationHistory(incoming(),'acme','hold-a',{before:7,head:9,revision})).rejects.toMatchObject({code:'NATIVE_QUALITY_RESPONSE_INVALID'});
  transport.mockResolvedValueOnce(ok({data:{items:[],nextBefore:null,headVersion:9,historyRevision:revision}}));
  await expect(client.cancellationHistory(incoming(),'acme','hold-a',{before:7,head:9,revision})).rejects.toMatchObject({code:'NATIVE_QUALITY_RESPONSE_INVALID'});
  await expect(client.cancellationHistory(incoming(),'acme','hold-a',{before:-0,head:9,revision})).rejects.toMatchObject({code:'NATIVE_QUALITY_REQUEST_INVALID'});
 });
 it('retains signed historic versions independently of the current hold version',async()=>{
  const revision=`qch1:${'a'.repeat(64)}`;
  const item={id:'later',holdId:'hold-a',version:12,action:'request',reason:'Historic',actorId:'maker',createdAt:1};
  const transport=vi.fn().mockResolvedValueOnce(ok({data:{items:[item,{...item,id:'zero',version:0}],nextBefore:0,headVersion:9,historyRevision:revision}}))
   .mockResolvedValueOnce(ok({data:{items:[{...item,id:'negative',version:-3}],nextBefore:null,headVersion:9,historyRevision:revision}}));
  const client=createNativeQualityReader(config(),transport);
  await expect(client.cancellationHistory(incoming(),'acme','hold-a')).resolves.toMatchObject({nextBefore:0,items:[{version:12},{version:0}]});
  await expect(client.cancellationHistory(incoming(),'acme','hold-a',{before:0,head:9,revision})).resolves.toMatchObject({items:[{version:-3}]});
  expect(transport.mock.calls[1][1].searchParams.get('before')).toBe('0');
 });
 it('forwards the configured Holding and current session to the workspace read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace}));
  const client=createNativeQualityReader(config(),transport);
  const result=await client.workspace(incoming(),'acme');
  expect(result.holds[0]).toMatchObject({id:'hold-a',status:'awaiting_release',decision:{canRelease:true},
    productName:'Packaged goods',warehouseName:'Main warehouse',trackingMode:'lot',lotCode:'LOT-1',
    expiryDate:'2027-01-01',eligibleCount:2,specId:'spec-1',specVersion:2,
    quantityScope:'partial',heldQuantityMilli:1000,sourceType:'goods_receipt'});
  expect(result.stock[0]).toMatchObject({warehouseId:'warehouse',productId:'product',quantityMilli:2000,
    version:4,warehouseName:'Main warehouse',trackingMode:'lot'});
  expect(result.lots[0]).toMatchObject({id:'lot-1',balanceVersion:3,qualityHeld:false,lotCode:'LOT-1'});
  expect(result.products[0]).toEqual({id:'product',sku:'SKU-1',name:'Packaged goods'});
  expect(result.specifications[0]).toMatchObject({id:'spec-1',productId:'product',status:'active',
    version:2,sku:'SKU-1',productName:'Packaged goods'});
  expect(result.specMetrics[0]).toMatchObject({id:'metric-weight',specId:'spec-1',minimum:'9.9',maximum:'10.1'});
  expect(result.canHold).toBe(true);expect(result.canManageSpecs).toBe(true);
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
