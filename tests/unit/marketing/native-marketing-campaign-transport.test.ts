import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeMarketingCampaignClient,
  nativeMarketingCampaignAvailable,
  nativeMarketingCampaignConfiguration,
  NativeMarketingCampaignError,
  type NativeMarketingCampaignPageContext,
} from '../../../packages/feature-sales/src/public/native-marketing-campaign';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44996',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>= {}) => new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeMarketingCampaignPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['marketing'],iam:{permissions:['records.read']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['marketing'],permissions:['records.read'],
  expiresAt:9_007_199_254_740_991};
const campaign={
  id:'campaign-1',company_id:'company',code:'AUTUMN',name:'Autumn demand campaign',
  version:1,created_at:100,
};
const attributionRow={
  campaign:'AUTUMN',source:'web',captured:3,pending:1,handed_off:1,rejected:1,
};
const purchase={newWork:true,settleExisting:true,history:true,revision:'7'};
const page=(campaigns:unknown[],attribution:unknown[],overrides:Record<string,unknown>={})=>({
  data:{campaigns,attribution,purchase,...overrides},
  context,
});
const input={holdingSlug:'acme',
  expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Marketing campaign transport',()=>{
 it('forwards the configured Holding, session, CAS and exact report route',async()=>{
  const transport=vi.fn().mockResolvedValue(ok(page([campaign],[attributionRow])));
  const client=createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config()),transport);
  const result=await client.report(incoming(),actor,input);
  expect(result).toEqual({campaigns:[campaign],attribution:[attributionRow],purchase});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/marketing/campaign-attribution');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace slug',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({
    data:{campaigns:[],attribution:[],purchase:null},context:branchContext,
  }));
  const client=createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config()),transport);
  const result=await client.report(incoming(),branchActor,{...input,branchSlug:'north'});
  expect(result.campaigns).toEqual([]);
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/marketing/campaign-attribution');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('x-erpx-workspace-department')).toBeNull();
 });

 it('forwards the selected department workspace slugs',async()=>{
  const departmentActor={...actor,branchId:'branch',departmentId:'department',
    scope:{kind:'department' as const,branchSlug:'north',departmentSlug:'ops'}};
  const departmentContext={...context,branchId:'branch',departmentId:'department',
    branchSlug:'north',departmentSlug:'ops'};
  const transport=vi.fn().mockResolvedValue(ok({
    data:{campaigns:[],attribution:[],purchase:null},context:departmentContext,
  }));
  const client=createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config()),transport);
  await client.report(incoming(),departmentActor,{...input,branchSlug:'north',departmentSlug:'ops'});
  const [,url,headers]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44996/api/v1/holdings/acme/marketing/campaign-attribution');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(headers.get('x-erpx-workspace-department')).toBe('ops');
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'PACKAGE_REQUIRED',message:'Marketing history is required.'}}),
    {status:403,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config()),transport);
  await expect(client.report(incoming(),actor,input)).rejects.toMatchObject({
    code:'PACKAGE_REQUIRED',status:403,message:'Marketing history is required.',
  });
 });

 it('rejects campaign, attribution, purchase, page and context mismatches',async()=>{
  const client=(value:unknown)=>createNativeMarketingCampaignClient(
    nativeMarketingCampaignConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  await expect(client(page([{...campaign,company_id:'foreign'}],[attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([{...campaign,code:'A'}],[attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([{...campaign,name:' '}],[attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([campaign,campaign],[attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([{...campaign,id:'campaign-2',code:'AUTUMN'},{...campaign,id:'campaign-1',code:'AUTUMN'}],[attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([campaign],[{...attributionRow,captured:4}]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([campaign],[attributionRow,attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([campaign],[{...attributionRow,captured:1,pending:0,handed_off:0,rejected:0},{...attributionRow,campaign:'ZETA',captured:2,pending:2,handed_off:0,rejected:0}]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client(page([campaign],[attributionRow],{purchase:{newWork:true,settleExisting:true,history:true}}))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
  await expect(client({data:page([campaign],[attributionRow]).data,context:{...context,branchId:'branch',branchSlug:'north'}})
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED'});
  await expect(client({data:page([campaign],[attributionRow]).data,context:{...context,revision:8}})
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED'});
  await expect(client(page([{...campaign,extra:'field'}],[attributionRow]))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
 });

 it('bounds an oversized owner response and rejects a non-JSON success body',async()=>{
  const client=(response:Response)=>createNativeMarketingCampaignClient(
    nativeMarketingCampaignConfiguration(config()),
    vi.fn().mockResolvedValue(response),
  );
  await expect(client(new Response('x'.repeat(2*1024*1024+1),{status:200,headers:{'content-type':'application/json'}}))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_LIMIT'});
  await expect(client(new Response('{}',{status:200,headers:{'content-type':'text/plain'}}))
    .report(incoming(),actor,input)).rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_RESPONSE_INVALID'});
 });

 it('rejects a host or actor that is not the configured exact authority',async()=>{
  const client=createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config()),vi.fn());
  await expect(client.report(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_AUTHORITY_INVALID'});
  await expect(client.report(incoming(),{...actor,holdingId:'h002'},input))
    .rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_AUTHORITY_INVALID'});
  await expect(client.report(incoming(),{...actor,authority:undefined},input))
    .rejects.toMatchObject({code:'NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED'});
 });

 it('fails closed when the native Marketing configuration is absent',async()=>{
  const previous=process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG;
  delete process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG;
  expect(nativeMarketingCampaignAvailable()).toBe(false);
  const {nativeMarketingCampaignReport}=await import('../../../packages/feature-sales/src/public/native-marketing-campaign-page');
  await expect(nativeMarketingCampaignReport(actor as never)).resolves.toBeUndefined();
  if(previous!==undefined)process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG=previous;
 });

 it('does not fall back when a configured native Marketing selector is malformed',async()=>{
  const previous=process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG;
  process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG='{not-json';
  const {nativeMarketingCampaignReport}=await import('../../../packages/feature-sales/src/public/native-marketing-campaign-page');
  await expect(nativeMarketingCampaignReport(actor as never)).rejects.toMatchObject({
    code:'NATIVE_MARKETING_CAMPAIGN_CONFIGURATION_INVALID',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG;
  else process.env.ERPX_NATIVE_MARKETING_CAMPAIGN_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  for(const privateBaseUrl of ['http://127.0.0.1:3100','http://127.0.0.1:3101','http://127.0.0.1:3103','http://127.0.0.1:5432','http://127.0.0.1:8080','http://127.0.0.1:3200','http://10.0.0.1:44996','https://127.0.0.1:44996','http://127.0.0.1:44996/']) {
    expect(()=>createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config({privateBaseUrl})),vi.fn())).toThrow();
  }
  for(const holdings of [
    [{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
    [{origin:'https://acme.example.com:8443',holdingSlug:'acme',holdingId:'h001'}],
    [{origin:'https://acme.example.com',holdingSlug:'Acme',holdingId:'h001'}],
  ]) {
    expect(()=>createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config({holdings})),vi.fn())).toThrow();
  }
  expect(()=>nativeMarketingCampaignConfiguration('{not-json')).toThrow();
  expect(()=>nativeMarketingCampaignConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativeMarketingCampaignClient(nativeMarketingCampaignConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',async()=>{
  const {nativeMarketingCampaignFailure}=await import('../../../packages/feature-sales/src/public/native-marketing-campaign-page');
  expect(nativeMarketingCampaignFailure(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeMarketingCampaignFailure(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'MARKETING_CAMPAIGN_REQUEST_INVALID'});
  expect(nativeMarketingCampaignFailure(new NativeMarketingCampaignError('NATIVE_MARKETING_CAMPAIGN_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_MARKETING_CAMPAIGN_UNAVAILABLE'});
  expect(nativeMarketingCampaignFailure(new NativeMarketingCampaignError('PACKAGE_REQUIRED',403,'History required.')))
    .toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  expect(nativeMarketingCampaignFailure(new NativeMarketingCampaignError('MARKETING_CAMPAIGN_SOURCE_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'MARKETING_CAMPAIGN_SOURCE_CHANGED'});
  expect(nativeMarketingCampaignFailure(new Error('other'))).toBeUndefined();
 });
});
