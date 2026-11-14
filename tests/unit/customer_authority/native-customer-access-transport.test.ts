import {describe,it,expect,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {
  createNativeCustomerAccessClient,
  nativeCustomerAccessAvailable,
  nativeCustomerAccessConfiguration,
  NativeCustomerAccessError,
  type NativeCustomerAccessPageContext,
} from '../../../packages/feature-sales/src/public/native-customer-access';
import { nativeCustomerAccessCommand, nativeCustomerAccessFailure, nativeCustomerAccessWorkspace } from '../../../packages/feature-sales/src/public/native-customer-access-page';

const config=(overrides:Record<string,unknown>={})=>JSON.stringify({
  privateBaseUrl:'http://127.0.0.1:44993',
  timeoutMs:1000,
  holdings:[{origin:'https://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  ...overrides,
});
const incoming=(overrides:Record<string,string>={})=>new Headers({
  host:'acme.example.com',
  cookie:'__Host-erpx-product=session',
  ...overrides,
});
const actor:NativeCustomerAccessPageContext={
  userId:'user',holdingId:'h001',holdingSlug:'acme',companyId:'company',
  branchId:null,departmentId:null,assignmentId:'assignment',role:'owner',
  assignmentScope:'holding',scope:{kind:'holding'},authority:{revision:7,assignmentVersion:3},
  modules:['sales'],iam:{permissions:['commerce.read','commerce.sales','commerce.approve']},
};
const context={userId:'user',holdingId:'h001',companyId:'company',branchId:null,
  departmentId:null,branchSlug:null,departmentSlug:null,assignmentId:'assignment',
  assignmentVersion:3,revision:7,role:'owner',assignmentScopeKind:'holding',
  modules:['sales'],permissions:['commerce.read','commerce.sales','commerce.approve'],
  expiresAt:9_007_199_254_740_991};
const row={id:'customer-access-review-1',company_id:'company',order_id:'order-1',
  customer_id:'customer-1',email:'customer@example.test',expires_at:1_800_086_400_000,
  status:'pending',reason:'Verified source-bound customer authority',created_by:'requester',
  approved_by:null,reviewed_at:null,version:1,created_at:1_799_999_999_000,
  updated_at:1_799_999_999_000,number:'QT-100',order_state:'confirmed',branch_id:null,
  department_id:null,reviewers:1,canReview:true,canReject:true,canRevoke:true,canInvite:false};
const source={id:'order-1',number:'QT-100',name:'Synthetic Customer',version:3};
const event={version:1,action:'request',actor_id:'requester',
  reason:'Verified source-bound customer authority',created_at:1_799_999_999_000};
const read={binding_version:1,user_id:'portal-user',read_at:1_799_999_998_000,
  projection_version:'v1'};
const input={holdingSlug:'acme',expected:{companyId:'company',assignmentId:'assignment',revision:7,assignmentVersion:3}};
const ok=(value:unknown)=>new Response(JSON.stringify(value),{status:200,headers:{'content-type':'application/json'}});

describe('native Customer Authority customer-access transport',()=>{
 it('accepts one bounded contributor Holding port and prefers the forwarded host',async()=>{
  const contributor={...JSON.parse(config()),holdings:[{origin:'https://acme.example.com:3443',holdingSlug:'acme',holdingId:'h001'}]};
  const transport=vi.fn().mockResolvedValue(ok({data:{rows:[row],sources:[source]},context}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(JSON.stringify(contributor)),transport);
  await access.workspace(incoming({host:'127.0.0.1:3206','x-forwarded-host':'acme.example.com:3443'}),actor,input);
  expect(transport.mock.calls[0][2].get('host')).toBe('acme.example.com:3443');
  for(const origin of ['https://acme.example.com:0','https://acme.example.com:65536','https://acme.example.com:abc'])
    expect(()=>createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config({holdings:[{origin,holdingSlug:'acme',holdingId:'h001'}]})),vi.fn())).toThrow();
 });

 it('forwards the exact workspace Host, session and context CAS',async()=>{
  const transport=vi.fn().mockResolvedValue(ok({data:{rows:[row],sources:[source]},context}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),transport);
  const result=await access.workspace(incoming(),actor,input);
  expect(result.rows).toEqual([row]);
  expect(result.sources).toEqual([source]);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/customer-access');
  expect(headers.get('host')).toBe('acme.example.com');
  expect(headers.get('cookie')).toBe('__Host-erpx-product=session');
  expect(headers.get('x-erpx-context-company')).toBe('company');
  expect(headers.get('x-erpx-context-assignment')).toBe('assignment');
  expect(headers.get('x-erpx-context-revision')).toBe('7');
  expect(headers.get('x-erpx-context-assignment-version')).toBe('3');
  expect(headers.get('idempotency-key')).toBeNull();
  expect(body).toBeUndefined();
 });

 it('forwards the selected branch workspace and exact detail path',async()=>{
  const branchActor={...actor,branchId:'branch',scope:{kind:'branch' as const,branchSlug:'north'}};
  const branchContext={...context,branchId:'branch',branchSlug:'north'};
  const transport=vi.fn().mockResolvedValue(ok({data:{binding:{...row,branch_id:'branch'},events:[event],reads:[read]},context:branchContext}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),transport);
  await access.detail(incoming(),branchActor,'customer-access-review-1',{...input,branchSlug:'north'});
  const [method,url,headers]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/customer-access/customer-access-review-1');
  expect(headers.get('x-erpx-workspace-branch')).toBe('north');
 });

 it('forwards the exact source-bound request and decision commands',async()=>{
  const transport=vi.fn()
    .mockResolvedValueOnce(ok({data:{id:'customer-access-review-1',status:'pending',version:1},context}))
    .mockResolvedValueOnce(ok({data:{id:'customer-access-review-1',status:'approved',version:2},context}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),transport);
  const request={orderId:'order-1',sourceVersion:3,email:'customer@example.test',expiresAt:1_800_086_400_000,reason:'Verified source'};
  expect(await access.request(incoming(),actor,JSON.stringify(request),'customer-access-command-1',input))
    .toEqual({id:'customer-access-review-1',status:'pending',version:1});
  const [,url,headers,body]=transport.mock.calls[0];
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/customer-access');
  expect(headers.get('idempotency-key')).toBe('customer-access-command-1');
  expect(headers.get('content-type')).toBe('application/json');
  expect(JSON.parse(body!)).toEqual(request);
  expect(await access.change(incoming(),actor,'customer-access-review-1',JSON.stringify({action:'approve',expectedVersion:1,reason:'Independent review'}),'customer-access-command-2',input))
    .toEqual({id:'customer-access-review-1',status:'approved',version:2});
  const [secondMethod,secondUrl,secondHeaders,secondBody]=transport.mock.calls[1];
  expect(secondMethod).toBe('POST');
  expect(secondUrl.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/customer-access/customer-access-review-1');
  expect(secondHeaders.get('idempotency-key')).toBe('customer-access-command-2');
  expect(JSON.parse(secondBody!)).toEqual({action:'approve',expectedVersion:1,reason:'Independent review'});
 });

 it('surfaces a bounded owner error with its status, code and message',async()=>{
  const transport=vi.fn().mockResolvedValue(new Response(
    JSON.stringify({error:{code:'CUSTOMER_ACCESS_CHANGED',message:'Reload the current access decision.'}}),
    {status:409,headers:{'content-type':'application/json'}},
  ));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),transport);
  await expect(access.change(incoming(),actor,'customer-access-review-1','{}','customer-access-command-3',input))
    .rejects.toMatchObject({code:'CUSTOMER_ACCESS_CHANGED',status:409,message:'Reload the current access decision.'});
 });

 it('rejects foreign scope, duplicate rows, extra keys and changed context',async()=>{
  const foreign=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{rows:[{...row,company_id:'foreign'}],sources:[]},context}),
  ));
  await expect(foreign.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID'});
  const duplicate=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{rows:[row,row],sources:[]},context}),
  ));
  await expect(duplicate.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID'});
  const extra=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{binding:{...row,extra:'field'},events:[event],reads:[read]},context}),
  ));
  await expect(extra.detail(incoming(),actor,'customer-access-review-1',input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID'});
  const changed=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),vi.fn().mockResolvedValue(
    ok({data:{rows:[],sources:[]},context:{...context,revision:8}}),
  ));
  await expect(changed.workspace(incoming(),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED'});
  const unknownHost=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),vi.fn());
  await expect(unknownHost.workspace(incoming({host:'evil.example.com'}),actor,input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_AUTHORITY_INVALID'});
 });

 it('fails closed when the native Customer Authority configuration is absent',()=>{
  const previous=process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  delete process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  expect(nativeCustomerAccessAvailable()).toBe(false);
  if(previous!==undefined)process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG=previous;
 });

 it('retains access fallback without configuration but fails Portal invitations closed',async()=>{
  const previous=process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  delete process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  expect(await nativeCustomerAccessCommand(actor as never,{id:'binding',action:'approve',expectedVersion:1,reason:'Review'},'customer-access-command-9')).toBeUndefined();
  await expect(nativeCustomerAccessCommand(actor as never,{id:'binding',action:'invite',expectedVersion:2,reason:'Invite'},'customer-access-command-9')).rejects.toMatchObject({
    status:503,code:'NATIVE_CUSTOMER_ACCESS_CONFIGURATION_REQUIRED',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;else process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG=previous;
 });

 it('posts one Portal invitation command with its exact body and idempotency key',async()=>{
  const previous=process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG=config();
  const invitation={id:'binding-1',bindingId:'binding-1',invitationId:'customer-portal-invitation-1',generation:2,version:4,state:'queued',expiresAt:1_800_000_000_000};
  const transport=vi.fn().mockResolvedValue(ok({data:invitation,context}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),transport);
  const result=await access.invitation(incoming(),actor,'binding-1',JSON.stringify({action:'resend',expectedVersion:4,reason:'Replace the first invitation'}),'customer-access-command-10',input);
  expect(result).toEqual(invitation);
  const [method,url,headers,body]=transport.mock.calls[0];
  expect(method).toBe('POST');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/customer-access/binding-1/invitations');
  expect(headers.get('host')).toBe('acme.example.com');
  expect(headers.get('idempotency-key')).toBe('customer-access-command-10');
  expect(headers.get('content-type')).toBe('application/json');
  expect(JSON.parse(body)).toEqual({action:'resend',expectedVersion:4,reason:'Replace the first invitation'});
  if(previous===undefined)delete process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;else process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG=previous;
 });

 it('rejects an invalid invitation receipt and a history row from another binding',async()=>{
  const invitation={id:'binding-1',bindingId:'binding-1',invitationId:'customer-portal-invitation-1',generation:1,version:4,state:'queued',expiresAt:1_800_000_000_000};
  const bad=vi.fn().mockResolvedValue(ok({data:{...invitation,bindingId:'other'},context}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),bad);
  await expect(access.invitation(incoming(),actor,'binding-1','{}','customer-access-command-11',input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID'});
  const historyRow={id:'customer-portal-invitation-1',binding_id:'other',generation:1,holding_slug:'acme',email:'customer@example.test',state:'queued',token_expires_at:1_800_000_000_000,attempts:0,next_attempt_at:1_799_999_999_000,last_error:null,created_by:'requester',created_at:1_799_999_999_000,sent_at:null,accepted_at:null,accepted_user_id:null};
  const wrongBinding=vi.fn().mockResolvedValue(ok({data:{invitations:[historyRow],invitationEvents:[]},context}));
  const second=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),wrongBinding);
  await expect(second.invitationHistory(incoming(),actor,'binding-1',input))
    .rejects.toMatchObject({code:'NATIVE_CUSTOMER_ACCESS_RESPONSE_INVALID'});
 });

 it('reads the immutable invitation history for the exact binding',async()=>{
  const historyRow={id:'customer-portal-invitation-1',binding_id:'binding-1',generation:2,holding_slug:'acme',email:'customer@example.test',state:'queued',token_expires_at:1_800_000_000_000,attempts:0,next_attempt_at:1_799_999_999_000,last_error:null,created_by:'requester',created_at:1_799_999_999_000,sent_at:null,accepted_at:null,accepted_user_id:null};
  const superseded={...historyRow,id:'customer-portal-invitation-0',generation:1,state:'superseded',last_error:'SUPERSEDED'};
  const historyEvent={id:'customer-portal-invite-event-1',invitation_id:'customer-portal-invitation-1',action:'issued',actor_id:'requester',reason:'Invite',created_at:1_799_999_999_000};
  const transport=vi.fn().mockResolvedValue(ok({data:{invitations:[historyRow,superseded],invitationEvents:[historyEvent]},context}));
  const access=createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config()),transport);
  const result=await access.invitationHistory(incoming(),actor,'binding-1',input);
  expect(result.invitations.map(item=>item.generation)).toEqual([2,1]);
  expect(result.invitationEvents).toEqual([historyEvent]);
  const [method,url]=transport.mock.calls[0];
  expect(method).toBe('GET');
  expect(url.toString()).toBe('http://127.0.0.1:44993/api/v1/holdings/acme/customer-access/binding-1/invitations');
 });

 it('maps a malformed configured native selector to a bounded 503 without fallback',async()=>{
  const previous=process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;
  process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG='{not-json';
  await expect(nativeCustomerAccessWorkspace(actor as never)).rejects.toMatchObject({
    status:503,code:'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE',
  });
  if(previous===undefined)delete process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG;else process.env.ERPX_NATIVE_CUSTOMER_ACCESS_CONFIG=previous;
 });

 it('rejects reserved loopback ports, insecure Holding origins and malformed configuration',()=>{
  expect(()=>createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config({privateBaseUrl:'http://127.0.0.1:3100'})),vi.fn())).toThrow();
  expect(()=>createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config({privateBaseUrl:'http://10.0.0.1:44993'})),vi.fn())).toThrow();
  expect(()=>createNativeCustomerAccessClient(nativeCustomerAccessConfiguration(config({
    holdings:[{origin:'http://acme.example.com',holdingSlug:'acme',holdingId:'h001'}],
  })),vi.fn())).toThrow();
  expect(()=>nativeCustomerAccessConfiguration('{not-json')).toThrow();
 });

 it('maps transport failures to the retained page/route status policy',()=>{
  expect(nativeCustomerAccessFailure(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_CONTEXT_CHANGED')))
    .toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
  expect(nativeCustomerAccessFailure(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_REQUEST_INVALID')))
    .toMatchObject({status:422,code:'CUSTOMER_ACCESS_REQUEST_INVALID'});
  expect(nativeCustomerAccessFailure(new NativeCustomerAccessError('NATIVE_CUSTOMER_ACCESS_AUTHORITY_INVALID')))
    .toMatchObject({status:503,code:'NATIVE_CUSTOMER_ACCESS_UNAVAILABLE'});
  expect(nativeCustomerAccessFailure(new NativeCustomerAccessError('CUSTOMER_ACCESS_CHANGED',409,'Reload.')))
    .toMatchObject({status:409,code:'CUSTOMER_ACCESS_CHANGED'});
  expect(nativeCustomerAccessFailure(new Error('other'))).toBeUndefined();
 });
});
