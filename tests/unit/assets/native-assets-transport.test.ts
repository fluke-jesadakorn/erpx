import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
const purchaseOracle=vi.hoisted(()=>({run:vi.fn(async(_actor:unknown,command:unknown)=>command)}));
vi.mock('@erpx/server-assets/adapters/purchases',()=>({withAssetPurchasedCommand:purchaseOracle.run,withAssetPurchasedRead:vi.fn(),assetPurchaseControls:vi.fn()}));
import {createAsset as legacyCreateAsset,changeAsset as legacyChangeAsset} from '../../../packages/server-assets/src/adapters/assets';
import type {WorkContext} from '../../../packages/server-identity_access/src/public/authorization';
import {createNativeAssetsClient,type NativeAssetsPageContext} from '../../../packages/feature-assets/src/adapters/server/native-assets';
import {currencyScale,decimalUnits} from '../../../packages/feature-finance/src/lib/company-money';
import {createAssetRegistration} from '../../../packages/server-assets/src/application/registration';
import {createAssetSchema} from '../../../packages/server-assets/src/domain/assets';
import {ApiError,errorResponse} from '../../../packages/server-runtime/src/adapters/http';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44991',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  origin:'https://acme.example.com',
  ...overrides,
});
const actor:NativeAssetsPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},
  authority:{revision:7,assignmentVersion:3},modules:['assets'],
  iam:{permissions:['records.read','records.write']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,departmentId:null,
  branchSlug:null,departmentSlug:null,assignmentId:'assignment',assignmentVersion:3,revision:7,
  role:'owner',assignmentScopeKind:'holding',modules:['assets'],
  permissions:['records.read','records.write'],expiresAt:9_007_199_254_740_991};
const asset={id:'asset-1',company_id:'company',branch_id:'branch',department_id:null,code:'A-1',
  name:'Asset One',category:'equipment',currency:'THB',cost_minor:0,acquired_date:'2026-01-01',
  warranty_end:null,finance_asset_id:null,finance_asset_version:null,condition:'good',
  lifecycle_status:'registered',placement:null,custodian_id:null,custodian_name:null,
  retirement_reason:null,version:1,created_by:'user',created_at:0,updated_at:0,can_review:true};
const workspace={items:[asset],people:[{id:'user',name:'Asset Owner'}],
  nodes:[{id:'branch',kind:'branch',display_name:'Branch',branch_id:null}],
  financeAssets:[],canLinkFinance:false,canCreate:true,actorId:'user'};
const detail={...workspace,asset,finance:null,
  history:[{id:'event-1',asset_id:'asset-1',version:1,action:'register',actor_id:'user',
    reason:'Asset registered',condition:'good',lifecycle_status:'registered',placement:null,
    custodian_id:null,created_at:0}],
  maintenance:[]};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});
const readInput={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};

describe('native Assets transport',()=>{
 it('retains legacy registration money error responses before owner effects',async()=>{
  const prepare=vi.fn(),register=vi.fn();
  const run=createAssetRegistration({authorize:()=>{},authorizeFinanceLink:()=>{},parse:raw=>createAssetSchema.parse(raw),scope:{branchId:null,departmentId:null},clock:{now:()=>0},decimalUnits,
   repository:async()=>({replay:async()=>null,currencyScale:async currency=>currencyScale(currency),prepareAcquisitionCost:prepare,register}),
   error:(status,code,message)=>new ApiError(status,code,message)});
  for(const [currency,cost,status,code] of [['XAG','1.234',500,'INTERNAL_ERROR'],['USD','1.234',500,'INTERNAL_ERROR'],['USD','90071992547409.92',422,'MONEY_OVERFLOW']] as const){
   const input={branchId:'branch',code:'A',name:'Asset',category:'equipment',currency,cost,acquiredDate:'2026-01-01'};
   const error=await run(input,'legacy-oracle-key').then(()=>null,reason=>reason);
   expect(error).toBeInstanceOf(Error);const response=errorResponse(error);
   expect(response.status).toBe(status);expect((await response.json()).error.code).toBe(code);
  }
  expect(decimalUnits('1.2300',2)).toBe(123n);
  expect(prepare).not.toHaveBeenCalled();expect(register).not.toHaveBeenCalled();
 });
 it('forwards the configured Holding, session and current CAS to the workspace read',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:workspace,context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  const result=await client.read(incoming(),actor,readInput);
  expect(result.items[0]).toMatchObject({id:'asset-1',code:'A-1',condition:'good'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
 });

 it('reads detail by id and forwards branch/department workspace selectors',async()=>{
  const scoped=actor.scope.kind==='holding'
    ? {...actor,scope:{kind:'branch' as const,branchSlug:'branch'}}
    : actor;
  const transport=vi.fn().mockResolvedValue(ok({data:{...detail,asset:{...asset,branch_id:'branch'}},context:{...context,branchSlug:'branch'}}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await client.detail(incoming(),{...scoped,authority:actor.authority},
    {holdingSlug:'acme',id:'asset-1',branchSlug:'branch'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets?id=asset-1');
  expect(headers.get('x-erpx-workspace-branch')).toBe('branch');
 });

 it('sends one keyed command with origin and returns the owner acknowledgement',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'asset-1',version:2,status:'in_service'},context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  const result=await client.command(incoming(),actor,{
    holdingSlug:'acme',
    body:{id:'asset-1',action:'condition',expectedVersion:1,condition:'degraded',reason:'Inspected'},
    idempotencyKey:'asset-command-1',
    origin:'https://acme.example.com',
  });
  expect(result).toEqual({id:'asset-1',version:2,status:'in_service'});
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44991/api/v1/holdings/acme/assets');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('idempotency-key')).toBe('asset-command-1');
  expect(headers.get('content-type')).toBe('application/json');
  expect(JSON.parse(body)).toMatchObject({id:'asset-1',action:'condition',expectedVersion:1});
 });

 it('accepts the retained create acknowledgement without fabricating a status',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{id:'asset-create-1',version:1},context}));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{holdingSlug:'acme',body:{code:'A'},idempotencyKey:'asset-create-1'}))
    .resolves.toEqual({id:'asset-create-1',version:1});
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'ASSET_FINANCE_DISPOSAL_REQUIRED',message:'Finance must post the linked asset disposal before physical retirement can be approved.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{id:'asset-1',action:'retire-approve',expectedVersion:2,reason:'Approve'},
    idempotencyKey:'asset-command-2',origin:'https://acme.example.com',
  })).rejects.toMatchObject({
    code:'ASSET_FINANCE_DISPOSAL_REQUIRED',
    status:409,
    message:'Finance must post the linked asset disposal before physical retirement can be approved.',
  });
 });

 it('rejects an unconfigured host, a changed context and a malformed response before returning data',async()=>{
  const client=createNativeAssetsClient(JSON.parse(config()),vi.fn());
  await expect(client.read(incoming({host:'evil.example.com'}),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_ASSETS_AUTHORITY_INVALID'});
  const changed=createNativeAssetsClient(JSON.parse(config()),vi.fn().mockResolvedValue(
    ok({data:workspace,context:{...context,revision:8}}),
  ));
  await expect(changed.read(incoming(),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_ASSETS_CONTEXT_CHANGED'});
  const malformed=createNativeAssetsClient(JSON.parse(config()),vi.fn().mockResolvedValue(
    ok({data:{...workspace,items:[{...asset,extra:'field'}]},context}),
  ));
  await expect(malformed.read(incoming(),actor,readInput))
    .rejects.toMatchObject({code:'NATIVE_ASSETS_RESPONSE_INVALID'});
 });

 it('rejects reserved loopback ports, insecure Holding origins and an invalid command key',async()=>{
  expect(()=>createNativeAssetsClient(JSON.parse(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeAssetsClient(JSON.parse(config({privateBaseUrl:'http://10.0.0.1:44991'})),vi.fn())).toThrow();
  expect(()=>createNativeAssetsClient(JSON.parse(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  const transport=vi.fn();
  const client=createNativeAssetsClient(JSON.parse(config()),transport);
  await expect(client.command(incoming(),actor,{
    holdingSlug:'acme',body:{},idempotencyKey:'short',origin:'https://acme.example.com',
  })).rejects.toMatchObject({code:'NATIVE_ASSETS_COMMAND_KEY_INVALID'});
  expect(transport).not.toHaveBeenCalled();
 });
});


describe('legacy Asset purchase routing oracle',()=>{
 it('derives create and every valid action from parsed owner input',async()=>{
  const context=actor as unknown as WorkContext;
  const create={branchId:'branch',code:'A',name:'Asset',category:'equipment',currency:'THB',costMinor:0,acquiredDate:'2026-01-01'};
  expect(await legacyCreateAsset(context,create,'key')).toMatchObject({operation:'asset.create',capability:'assets.register',purpose:'new-work',creating:true,permissions:['records.write']});
  expect(await legacyCreateAsset(context,{...create,financeAssetId:'finance-1',financeAssetVersion:2},'key')).toMatchObject({permissions:['records.write','finance.write']});
  for(const [body,capability,purpose,permissions] of [
   [{action:'place',expectedVersion:1,placement:'Desk',custodianId:'user',reason:'Place'},'assets.custody','new-work',['records.write']],
   [{action:'condition',expectedVersion:1,condition:'good',reason:'Inspect'},'assets.register','settle-existing',['records.write']],
   [{action:'retire-request',expectedVersion:1,reason:'Retire'},'assets.register','settle-existing',['records.write']],
   [{action:'retire-approve',expectedVersion:1,reason:'Approve'},'assets.register','settle-existing',['records.write','records.review']],
   [{action:'link-finance',expectedVersion:1,financeAssetId:'finance-1',financeAssetVersion:2,reason:'Link'},'assets.register','settle-existing',['records.write','finance.write']],
  ] as const) expect(await legacyChangeAsset(context,'asset-1',body,'key')).toMatchObject({operation:`asset.${body.action}`,capability,purpose,permissions});
  purchaseOracle.run.mockClear();
  await expect(legacyChangeAsset(context,'asset-1',{action:'unknown',expectedVersion:1,reason:'Bad'},'key')).rejects.toThrow();
  expect(purchaseOracle.run).not.toHaveBeenCalled();
 });
});
