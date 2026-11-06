import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativePosCashClient,
  nativePosCashAvailable,
  nativePosCashConfiguration,
  NativePosCashError,
  type NativePosCashActor,
} from '../../../packages/feature-pos/src/adapters/server/native-pos-cash';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44994',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  origin:'https://acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativePosCashActor={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:'branch',departmentId:null,assignmentId:'assignment',role:'cashier',
  assignmentScope:'branch',scope:{kind:'branch',branchSlug:'north'},
  authority:{revision:7,assignmentVersion:3},
  modules:['pos'],iam:{permissions:['pos.sell']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:'branch',
  departmentId:null,branchSlug:'north',departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'cashier',assignmentScopeKind:'branch',
  modules:['pos'],permissions:['pos.sell'],expiresAt:9_007_199_254_740_991};
const body=JSON.stringify({id:'shift-1',expectedVersion:3,amountMinor:50000,reason:'Opening float'});
const receipt={id:'0f8fad5b-d9cb-469f-a165-70867728950e'};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native POS cash command transport',()=>{
 it('forwards the exact Holding, session, context and original command body',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:receipt,context}));
  const client=createNativePosCashClient(nativePosCashConfiguration(config()),transport);
  const result=await client.cash(incoming(),actor,'cash-command-0001',body);
  expect(result).toEqual(receipt);
  const [method,url,headers,sent]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/pos/cash/create');
  expect(headers.get('host')).toBe('acme.example.com');
  expect(headers.get('origin')).toBe('https://acme.example.com');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('idempotency-key')).toBe('cash-command-0001');
  expect(headers.get('content-type')).toBe('application/json');
  expect(headers.get('x-erpx-context-user')).toBe('user');
  expect(headers.get('x-erpx-context-holding')).toBe('h001');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(JSON.parse(sent)).toEqual(JSON.parse(body));
 });

 it('surfaces a bounded owner error with status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'POS_VERSION_CONFLICT',message:'Load the latest data.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const client=createNativePosCashClient(nativePosCashConfiguration(config()),transport);
  await expect(client.cash(incoming(),actor,'cash-command-0001',body)).rejects.toMatchObject({
    code:'POS_VERSION_CONFLICT',status:409,message:'Load the latest data.',
  });
 });

 it('rejects malformed commands, authority and response shapes before returning success',async()=>{
  const client=(value:unknown)=>createNativePosCashClient(
    nativePosCashConfiguration(config()),
    vi.fn().mockResolvedValue(ok(value)),
  );
  await expect(client({data:receipt,context:{...context,revision:8}})
    .cash(incoming(),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_CONTEXT_CHANGED'});
  await expect(client({data:{...receipt,version:4},context})
    .cash(incoming(),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_RESPONSE_INVALID'});
  await expect(client({data:{id:'not-a-uuid'},context})
    .cash(incoming(),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_RESPONSE_INVALID'});
  await expect(client({data:receipt,context,extra:true})
    .cash(incoming(),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_RESPONSE_INVALID'});
  await expect(client({data:receipt,context})
    .cash(incoming(),actor,'short',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_REQUEST_INVALID'});
  for(const value of [
    '{"id":"shift-1","expectedVersion":3,"amountMinor":50000}',
    '{"id":"shift-1","expectedVersion":3,"amountMinor":0,"reason":"Opening float"}',
    '{"id":"shift-1","expectedVersion":0,"amountMinor":50000,"reason":"Opening float"}',
    '{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"a"}',
    '{"id":"shift-1","expectedVersion":3,"amountMinor":50000,"reason":"Opening float","extra":1}',
    '[]',
  ])
    await expect(client({data:receipt,context})
      .cash(incoming(),actor,'cash-command-0001',value))
      .rejects.toMatchObject({code:'NATIVE_POS_CASH_REQUEST_INVALID'});
  const noPermission={...actor,iam:{permissions:[]}};
  await expect(client({data:receipt,context})
    .cash(incoming(),noPermission,'cash-command-0001',body))
    .rejects.toMatchObject({code:'POS_PERMISSION_REQUIRED',status:403});
  const noModule={...actor,modules:[]};
  await expect(client({data:receipt,context})
    .cash(incoming(),noModule,'cash-command-0001',body))
    .rejects.toMatchObject({code:'POS_MODULE_FORBIDDEN',status:403});
  const department={...actor,departmentId:'department'};
  await expect(client({data:receipt,context})
    .cash(incoming(),department,'cash-command-0001',body))
    .rejects.toMatchObject({code:'POS_BRANCH_REQUIRED',status:403});
  const noAuthority={...actor,authority:undefined};
  await expect(client({data:receipt,context})
    .cash(incoming(),noAuthority,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_REQUEST_INVALID'});
 });

 it('rejects a Host or origin that is not the configured exact authority',async()=>{
  const client=createNativePosCashClient(
    nativePosCashConfiguration(config()),
    vi.fn().mockRejectedValue(new Error('offline')),
  );
  await expect(client.cash(incoming({host:'evil.example.com'}),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_AUTHORITY_INVALID'});
  await expect(client.cash(incoming({origin:'https://evil.example.com'}),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'ORIGIN_FORBIDDEN'});
  const wrongHolding={...actor,holdingId:'h002'};
  await expect(client.cash(incoming(),wrongHolding,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_AUTHORITY_INVALID'});
  await expect(client.cash(incoming(),actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_UNAVAILABLE'});
  const oversized={...incoming(),get:(name:string)=>name==='cookie'?'x'.repeat(8193):incoming().get(name)};
  await expect(client.cash(oversized,actor,'cash-command-0001',body))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_COOKIE_LIMIT'});
 });

 it('fails closed when the native POS cash configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_POS_CASH_CONFIG;
  delete process.env.ERPX_NATIVE_POS_CASH_CONFIG;
  expect(nativePosCashAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_POS_CASH_CONFIG=previous;
  process.env.ERPX_NATIVE_POS_CASH_CONFIG='{}';
  expect(nativePosCashAvailable()).toBe(true);
  delete process.env.ERPX_NATIVE_POS_CASH_CONFIG;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  for(const base of [
    'http://127.0.0.1:3100','http://127.0.0.1:3101','http://127.0.0.1:3103',
    'http://127.0.0.1:3000','http://127.0.0.1:8080','http://10.0.0.1:44994',
    'http://127.0.0.1:44994/path',
  ])
    expect(()=>createNativePosCashClient(nativePosCashConfiguration(config({privateBaseUrl:base})),vi.fn())).toThrow();
  expect(()=>createNativePosCashClient(nativePosCashConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>createNativePosCashClient(nativePosCashConfiguration(config({
    holdings:[
      {origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'},
      {origin:'https://north.example.com',holdingSlug:'acme',holdingId:'h002'},
    ],
  })),vi.fn())).toThrow();
  expect(()=>nativePosCashConfiguration('{not-json')).toThrow();
  expect(()=>nativePosCashConfiguration(config({timeoutMs:249}))).not.toThrow();
  expect(()=>createNativePosCashClient(nativePosCashConfiguration(config({timeoutMs:249})),vi.fn())).toThrow();
 });

 it('maps transport failures to the retained POS route status policy',async()=>{
  const {nativePosCashFailure}=await import('../../../packages/feature-pos/src/adapters/server/native-pos-cash');
  expect(nativePosCashFailure(new NativePosCashError('NATIVE_POS_CASH_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativePosCashFailure(new NativePosCashError('NATIVE_POS_CASH_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'POS_CASH_REQUEST_INVALID'});
  expect(nativePosCashFailure(new NativePosCashError('NATIVE_POS_CASH_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_POS_CASH_UNAVAILABLE'});
  expect(nativePosCashFailure(new NativePosCashError('NATIVE_POS_CASH_CONFIGURATION_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_POS_CASH_UNAVAILABLE'});
  expect(nativePosCashFailure(new NativePosCashError('POS_VERSION_CONFLICT',409,'Changed.')))
    .toMatchObject({status:409,code:'POS_VERSION_CONFLICT'});
  expect(nativePosCashFailure(new Error('other'))).toBeUndefined();
 });

 it('forwards a native POS command to its documented owner endpoint',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({
    data:{id:'0f8fad5b-d9cb-469f-a165-70867728950e',version:2},
    context,
  }));
  const client=createNativePosCashClient(nativePosCashConfiguration(config()),transport);
  const commandBody=JSON.stringify({id:'0f8fad5b-d9cb-469f-a165-70867728950e',expectedVersion:1,attachmentSetIds:[]});
  const result=await client.command(incoming(),actor,'sale.cancel','pos-command-0002',commandBody);
  expect(result).toEqual({id:'0f8fad5b-d9cb-469f-a165-70867728950e',version:2});
  const [method,url,headers,sent]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44994/api/v1/holdings/acme/pos/commands/sale.cancel');
  expect(headers.get('idempotency-key')).toBe('pos-command-0002');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
  expect(JSON.parse(sent)).toEqual(JSON.parse(commandBody));
 });

 it('selects the native command permission and rejects unsupported operations and malformed acknowledgements',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({
    data:{id:'shift'},context,
  }));
  const client=createNativePosCashClient(nativePosCashConfiguration(config()),transport);
  await expect(client.command(incoming(),actor,'shift.review','pos-command-0003','{}'))
    .rejects.toMatchObject({code:'POS_PERMISSION_REQUIRED',status:403});
  const reviewer={...actor,iam:{permissions:['pos.approve']}};
  const reviewerClient=createNativePosCashClient(
    nativePosCashConfiguration(config()),
    vi.fn().mockResolvedValue(ok({
      data:{id:'shift',version:2},
      context:{...context,permissions:['pos.approve']},
    })),
  );
  await expect(reviewerClient.command(incoming(),reviewer,'shift.review','pos-command-0003','{"id":"shift","expectedVersion":1,"reason":"Review"}'))
    .resolves.toEqual({id:'shift',version:2});
  await expect(client.command(incoming(),actor,'payment.retry','pos-command-0003','{}'))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_REQUEST_INVALID'});
  await expect(client.command(incoming(),actor,'sale.cancel','pos-command-0003','{"id":"sale","expectedVersion":1}'))
    .rejects.toMatchObject({code:'NATIVE_POS_CASH_RESPONSE_INVALID'});
  expect(transport).toHaveBeenCalledTimes(1);
 });

 it('maps the closed native command set to retained package scopes',async()=>{
  const {NATIVE_POS_COMMANDS,nativePosCommandAvailable,nativePosCommandCapability,nativePosCommandScope,nativePosWorkflowOperation} =
    await import('../../../packages/feature-pos/src/adapters/server/native-pos-cash');
  expect([...NATIVE_POS_COMMANDS]).toEqual([
    'shift.open','shift.close','shift.review','cash.approve',
    'sale.create','sale.add','sale.update_line','sale.customer','sale.move_table','sale.remove',
    'sale.discount','sale.merge','sale.split','payment.create','sale.hold','sale.resume','sale.cancel',
    'refund.request','refund.reject',
  ]);
  expect(nativePosCommandCapability('shift.open')).toEqual({capability:'pos.shifts',purpose:'new-work'});
  expect(nativePosCommandCapability('shift.close')).toEqual({capability:'pos.shifts',purpose:'settle-existing'});
  expect(nativePosCommandCapability('cash.approve')).toEqual({capability:'pos.shifts',purpose:'settle-existing'});
  expect(nativePosCommandCapability('sale.create')).toEqual({capability:'pos.sales',purpose:'new-work'});
  expect(nativePosCommandCapability('sale.add')).toEqual({capability:'pos.sales',purpose:'new-work'});
  expect(nativePosCommandCapability('sale.update_line')).toEqual({capability:'pos.sales',purpose:'new-work'});
  for (const operation of ['sale.customer','sale.move_table','sale.remove','sale.discount','sale.merge','sale.split'])
    expect(nativePosCommandCapability(operation)).toEqual({capability:'pos.sales',purpose:'new-work'});
  expect(nativePosCommandCapability('sale.cancel')).toEqual({capability:'pos.sales',purpose:'settle-existing'});
  expect(nativePosCommandCapability('refund.request')).toEqual({capability:'pos.refunds',purpose:'settle-existing'});
  expect(nativePosCommandCapability('payment.create')).toEqual({capability:'pos.sales',purpose:'new-work'});
  expect(nativePosWorkflowOperation('payment.create')).toBe(true);
  expect(nativePosWorkflowOperation('refund.approve')).toBe(true);
  expect(nativePosWorkflowOperation('connection.oauth')).toBe(false);
  expect(nativePosWorkflowOperation('settings.save')).toBe(false);
  const previous=process.env.ERPX_NATIVE_POS_CASH_CONFIG;
  delete process.env.ERPX_NATIVE_POS_CASH_CONFIG;
  expect(nativePosCommandAvailable()).toBe(false);
  expect(nativePosCommandScope('shift.open')).toBeUndefined();
  process.env.ERPX_NATIVE_POS_CASH_CONFIG=config();
  expect(nativePosCommandScope('shift.open')).toEqual({capability:'pos.shifts',purpose:'new-work'});
  expect(nativePosCommandScope('payment.retry')).toBeUndefined();
  if(previous!==undefined)process.env.ERPX_NATIVE_POS_CASH_CONFIG=previous;
  else delete process.env.ERPX_NATIVE_POS_CASH_CONFIG;
 });
});
