import {describe,it,expect,vi} from 'vitest';
const {requestHeaders}=vi.hoisted(()=>({requestHeaders:vi.fn()}));
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:requestHeaders}));
import {createNativeSessionClient,createNextNativeSessionReader,NativeSessionReadError} from '@erpx/feature-runtime/public/server';
import type {NativeSessionFetch} from '@erpx/feature-runtime/public/server';
import type {AccountSessionResponse} from '@erpx/contracts/account-auth';
const configuration={privateBaseUrl:'http://127.0.0.1:43117',accountOrigin:'https://account.example.test',holdingOrigins:['https://acme.example.test','https://northstar.example.test'],timeoutMs:250} as const;
function session(id='alpha'):AccountSessionResponse{return {session:{id:'session-'+id,userId:id,expiresAt:'2026-10-27T12:00:00Z',createdAt:'2026-10-27T11:00:00Z',updatedAt:'2026-10-27T11:00:00Z'},user:{id,name:'Synthetic '+id,email:id+'@example.test',emailVerified:true,image:null}};}
function json(value:unknown,status=200){return new Response(JSON.stringify(value),{status,headers:{'content-type':'application/json','cache-control':'private, no-store'}});}
function incoming(host='acme.example.test',cookie='__Host-erpx-product=opaque-test-only'){return new Headers({host,cookie,authorization:'never-forward-this','x-forwarded-host':'never-trust-this'});}
describe('typed server-only native session transport',()=>{
 it('forwards only exact Host and opaque Cookie to explicit loopback and never caches or follows redirects',async()=>{
  const calls:{url:URL;init:RequestInit}[]=[];const transport:NativeSessionFetch=async(url,init)=>{calls.push({url,init});return json(session());};
  const read=createNativeSessionClient(configuration,transport);expect(await read(incoming())).toEqual(session());expect(calls).toHaveLength(1);
  expect(calls[0]!.url.href).toBe('http://127.0.0.1:43117/api/auth/get-session');expect(calls[0]!.init).toMatchObject({method:'GET',cache:'no-store',redirect:'error',credentials:'omit'});
  const headers=new Headers(calls[0]!.init.headers);expect([...headers.keys()].sort()).toEqual(['accept','cache-control','cookie','host']);expect(headers.get('host')).toBe('acme.example.test');expect(headers.get('cookie')).toBe('__Host-erpx-product=opaque-test-only');
 });
 it('rejects normalized/foreign/protected transport authorities before any IO',async()=>{
  const transport=vi.fn<NativeSessionFetch>(async()=>json(null));
  for(const privateBaseUrl of ['https://127.0.0.1:43117','http://localhost:43117','http://127.1:43117','http://127.0.0.1:3100','http://127.0.0.1:3101','http://127.0.0.1:3103','http://127.0.0.1:3204','http://127.0.0.1:5432','http://127.0.0.1:8080','http://user:password@127.0.0.1:43117','http://127.0.0.1:43117/','http://127.0.0.1:43117?'])expect(()=>createNativeSessionClient({...configuration,privateBaseUrl},transport)).toThrow(NativeSessionReadError);
  for(const accountOrigin of ['https://ACCOUNT.example.test','https://account.example.test/','https://account.example.test:443','http://account.example.test','https://account.example.test#'])expect(()=>createNativeSessionClient({...configuration,accountOrigin},transport)).toThrow(NativeSessionReadError);
  const read=createNativeSessionClient(configuration,transport);await expect(read(incoming('retired.example.test'))).rejects.toMatchObject({code:'NATIVE_SESSION_AUTHORITY_INVALID'});await expect(read(incoming('acme.example.test','x'.repeat(8193)))).rejects.toMatchObject({code:'NATIVE_SESSION_COOKIE_LIMIT'});expect(transport).not.toHaveBeenCalled();
 });
 it('keeps anonymous200 null distinct from unavailable/invalid/non200 responses',async()=>{
  expect(await createNativeSessionClient(configuration,async()=>json(null))(incoming())).toBeNull();
  for(const status of [201,403,404,500,503])await expect(createNativeSessionClient(configuration,async()=>json(null,status))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_UNAVAILABLE',status});
  await expect(createNativeSessionClient(configuration,async()=>new Response('{}',{headers:{'content-type':'text/plain'}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
  await expect(createNativeSessionClient(configuration,async()=>json({session:session().session,user:{...session().user,id:'other'}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
  await expect(createNativeSessionClient(configuration,async()=>json({...session(),actor:{permissions:['*']}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
  await expect(createNativeSessionClient(configuration,async()=>json({...session(),session:{...session().session,id:''}}))(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_INVALID'});
 });
 it('reads actual async Next request snapshots separately for each invocation instead of memoizing a prior user',async()=>{
  requestHeaders.mockResolvedValueOnce(incoming('account.example.test','first')).mockResolvedValueOnce(incoming('northstar.example.test','second'));
  const seen:string[]=[];const read=createNativeSessionClient(configuration,async(_url,init)=>{const cookie=new Headers(init.headers).get('cookie')!;seen.push(cookie);return json(session(cookie));});
  const current=createNextNativeSessionReader(read);expect((await current())?.user.id).toBe('first');expect((await current())?.user.id).toBe('second');expect(seen).toEqual(['first','second']);expect(requestHeaders).toHaveBeenCalledTimes(2);
 });
 it('caps streamed bytes before JSON and closes the owned body on overflow',async()=>{
  let cancelled=0;const response=new Response(new ReadableStream<Uint8Array>({start(controller){controller.enqueue(new Uint8Array(32769));},cancel(){cancelled++;}}),{headers:{'content-type':'application/json'}});
  await expect(createNativeSessionClient(configuration,async()=>response)(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_RESPONSE_LIMIT'});expect(cancelled).toBe(1);
 });
 it('propagates caller cancellation and the absolute deadline through the response body',async()=>{
  const controller=new AbortController();let cancelled=0;const response=()=>new Response(new ReadableStream<Uint8Array>({cancel(){cancelled++;}}),{headers:{'content-type':'application/json'}});
  const reading=createNativeSessionClient(configuration,async()=>response())(incoming(),controller.signal);controller.abort();await expect(reading).rejects.toMatchObject({code:'NATIVE_SESSION_ABORTED'});
  await expect(createNativeSessionClient(configuration,async()=>response())(incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_ABORTED'});expect(cancelled).toBe(2);
 });
});

describe('Identity application native session selection',()=>{
 it('maps the validated native session dates and identity without a token or an Actor',async()=>{
  const {createIdentitySessionAuthenticator}=await import('../../../packages/server-identity_access/src/adapters/native-session');
  const {createIdentitySession}=await import('@erpx/server-identity_access/application/authorization');
  const previous=process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;let captured:Pick<Headers,'get'>|undefined;
  try{process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG=JSON.stringify(configuration);const native=vi.fn(()=>async(headers:Pick<Headers,'get'>)=>{captured=headers;return session();});
   const result=await createIdentitySession({headers:async()=>incoming(),authenticate:createIdentitySessionAuthenticator(native)})();
   expect(result.session).toEqual({session:{id:'session-alpha',userId:'alpha',expiresAt:new Date('2026-10-27T12:00:00Z'),createdAt:new Date('2026-10-27T11:00:00Z'),updatedAt:new Date('2026-10-27T11:00:00Z')},user:{id:'alpha',name:'Synthetic alpha',email:'alpha@example.test',emailVerified:true,image:null}});
   expect(captured?.get('cookie')).toBe('__Host-erpx-product=opaque-test-only');expect(native).toHaveBeenCalledWith({...configuration,originProfile:'public'});expect(result.session).not.toHaveProperty('actor');expect(result.session.session).not.toHaveProperty('token');
  }finally{if(previous===undefined)delete process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;else process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG=previous;}
 });
 it('requires native configuration and preserves malformed, anonymous and unavailable failures without legacy fallback',async()=>{
  const {createIdentitySessionAuthenticator}=await import('../../../packages/server-identity_access/src/adapters/native-session');
  const {createIdentitySession}=await import('@erpx/server-identity_access/application/authorization');
  const previous=process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;
  try{delete process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;const native=vi.fn(()=>{throw Error('unexpected native');});const absent=createIdentitySessionAuthenticator(native);await expect(absent('https://acme.example.test',incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_CONFIGURATION_REQUIRED',status:503});expect(native).not.toHaveBeenCalled();
   process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG='{broken';const broken=createIdentitySessionAuthenticator();await expect(broken('https://acme.example.test',incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_CONFIGURATION_INVALID'});
   process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG=JSON.stringify(configuration);const anonymous=createIdentitySessionAuthenticator(()=>async()=>null);await expect(createIdentitySession({headers:async()=>incoming(),authenticate:anonymous})()).rejects.toThrow('UNAUTHENTICATED');
   const failed=createIdentitySessionAuthenticator(()=>async()=>{throw new NativeSessionReadError('NATIVE_SESSION_UNAVAILABLE',503);});await expect(failed('https://acme.example.test',incoming())).rejects.toMatchObject({code:'NATIVE_SESSION_UNAVAILABLE',status:503});
  }finally{if(previous===undefined)delete process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;else process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG=previous;}
 });
});

it('requires an explicit contributor profile for exact local authorities',async()=>{
 const local={...configuration,accountOrigin:'https://account.erpx.localhost:3443',holdingOrigins:['https://acme.erpx.localhost:3443','https://northstar.erpx.localhost:3443','https://erpx-internal.erpx.localhost:3443'],originProfile:'contributor' as const};
 const send=vi.fn<NativeSessionFetch>(async()=>json(null));const read=createNativeSessionClient(local,send);for(const host of ['account.erpx.localhost:3443','acme.erpx.localhost:3443','northstar.erpx.localhost:3443','erpx-internal.erpx.localhost:3443'])expect(await read(incoming(host))).toBeNull();
 await expect(read(incoming('acme.erpx.localhost:3444'))).rejects.toMatchObject({code:'NATIVE_SESSION_AUTHORITY_INVALID'});
 for(const origin of ['http://acme.erpx.localhost:3443','https://acme.erpx.localhost:3444','https://x.acme.erpx.localhost:3443','https://acme.erpx.localhost','https://account.erpx.localhost:3443'])expect(()=>createNativeSessionClient({...local,holdingOrigins:[origin]},send)).toThrow(NativeSessionReadError);
 expect(()=>createNativeSessionClient({...local,originProfile:'public'},send)).toThrow(NativeSessionReadError);
});
it('derives native session profile only from trusted development configuration',async()=>{
 const {createIdentitySessionAuthenticator}=await import('../../../packages/server-identity_access/src/adapters/native-session');const client=vi.fn((config:unknown)=>{void config;return async()=>null;});
 try{vi.stubEnv('ERPX_NATIVE_ACCOUNT_SESSION_CONFIG',JSON.stringify({...configuration,originProfile:'contributor'}));vi.stubEnv('NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE','container-localhost');vi.stubEnv('NODE_ENV','production');await createIdentitySessionAuthenticator(client)(undefined,incoming());expect(client.mock.calls.at(-1)?.[0]).toMatchObject({originProfile:'public'});vi.stubEnv('NODE_ENV','development');await createIdentitySessionAuthenticator(client)(undefined,incoming());expect(client.mock.calls.at(-1)?.[0]).toMatchObject({originProfile:'contributor'});}
 finally{vi.unstubAllEnvs();}
});

// Company/role bootstrap is a native Account dependency, not a grant constructed
// from Next's previous database-backed role picker.
describe('native Account role selection transport',()=>{
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingId:'h001',holdingSlug:'acme'}]};
 const identity={sessionId:'synthetic-session',userId:'alpha',companyId:'company'};
 const state=()=>({holdingId:'h001',...identity,expiresAt:Date.now()+60000,options:[],selection:null,valid:false,branchId:null,departmentId:null,branchSlug:null,departmentSlug:null});
 it('binds only current Account identity and forwards opaque cookies/selectors without serialized authority',async()=>{
  const {createNativeWorkRoleTransport}=await import('@erpx/server-identity_access/public/native-work-role-transport');
  const send=vi.fn(async(_input:Parameters<typeof import('@erpx/native-sdk/field-service-parts').nativeWorkRolesRequest>[0])=>{void _input;return {status:200,body:{data:state()}};} );
  const h=incoming();h.set('x-erpx-context-company','forged-other-company');h.set('x-erpx-context-assignment','forged-role');h.set('x-erpx-workspace-branch','bangkok');
  expect(await createNativeWorkRoleTransport(JSON.stringify(config),send)(h,identity,'initialize')).toMatchObject(identity);
  const call=send.mock.calls[0]![0] as unknown as {headers:Headers;mode:string;method:string};expect(call).toMatchObject({method:'GET',mode:'initialize'});
  expect([...call.headers.keys()].sort()).toEqual(['accept','cookie','host','x-erpx-context-company','x-erpx-workspace-branch']);expect(call.headers.get('x-erpx-context-company')).toBe('company');expect(call.headers.get('cookie')).toBe(h.get('cookie'));
 });
 it('requires exact mutation origin and forbids wrong targets, protected ports and oversized cookies before IO',async()=>{
  const {createNativeWorkRoleTransport}=await import('@erpx/server-identity_access/public/native-work-role-transport');const send=vi.fn(async()=>({status:200,body:{data:state()}}));
  for(const privateBaseUrl of ['http://127.0.0.1:3100','http://127.0.0.1:3101','http://127.0.0.1:3103','http://127.0.0.1:3203','https://127.0.0.1:43117','http://external.example.test:43117'])expect(()=>createNativeWorkRoleTransport(JSON.stringify({...config,privateBaseUrl}),send)).toThrow();
  const read=createNativeWorkRoleTransport(JSON.stringify(config),send);
  await expect(read(incoming('retired.example.test'),identity,'restore')).rejects.toMatchObject({code:'NATIVE_WORK_ROLE_TARGET_UNAVAILABLE'});
  await expect(read(incoming('acme.example.test','x'.repeat(8193)),identity,'restore')).rejects.toMatchObject({code:'NATIVE_WORK_ROLE_COOKIE_LIMIT'});
  await expect(read(incoming(),identity,'select','{}')).rejects.toMatchObject({code:'ORIGIN_FORBIDDEN'});
  const h=incoming();h.set('origin','https://northstar.example.test');await expect(read(h,identity,'initialize')).rejects.toMatchObject({code:'ORIGIN_FORBIDDEN'});expect(send).not.toHaveBeenCalled();
 });
 it('rejects a response from a stale account or Company and preserves native CAS/session errors',async()=>{
  const {createNativeWorkRoleTransport}=await import('@erpx/server-identity_access/public/native-work-role-transport');
  for(const delta of [{companyId:'another-company'},{userId:'beta'},{sessionId:'another-session'},{holdingId:'h002'},{selection:{assignmentId:'role',assignmentVersion:1,revision:-1,expiresAt:Date.now()+1}}])await expect(createNativeWorkRoleTransport(JSON.stringify(config),async()=>({status:200,body:{data:{...state(),...delta}}}))(incoming(),identity,'restore')).rejects.toMatchObject({code:'NATIVE_WORK_ROLE_RESPONSE_INVALID'});
  for(const [status,code]of [[409,'WORK_CONTEXT_CHANGED'],[401,'SESSION_REVOKED']]as const)await expect(createNativeWorkRoleTransport(JSON.stringify(config),async()=>({status,body:{error:{code,message:'Synthetic native failure'}}}))(incoming(),identity,'restore')).rejects.toMatchObject({status,code});
 });
});

it('discovers native Companies before Company selection without forwarding a picker grant or Company authority',async()=>{
 const {createNativeAccountCompaniesReader}=await import('../../../packages/server-identity_access/src/adapters/native-account-companies');
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}]};
 const identity={sessionId:'synthetic-session',userId:'alpha'};
 const view={holdingId:'h001',...identity,name:'Synthetic account',email:'alpha@example.test',expiresAt:Date.now()+60000,companies:[{id:'company',slug:'company',name:'Synthetic Company'}]};
 const send=vi.fn(async(input:Parameters<typeof import('@erpx/native-sdk/field-service-parts').nativeAccountCompaniesRequest>[0])=>{expect([...input.headers.keys()].sort()).toEqual(['accept','cookie','host']);return {status:200,body:{data:view}};});
 const h=incoming();h.set('x-erpx-context-company','foreign-picker-company');h.set('x-erpx-context-assignment','forged-picker-role');
 expect(await createNativeAccountCompaniesReader(JSON.stringify(config),send)(h,identity,'h001')).toEqual(view);expect(send).toHaveBeenCalledTimes(1);
 for(const delta of [{holdingId:'h002'},{userId:'beta'},{sessionId:'other-session'},{companies:[view.companies[0],view.companies[0]]}])await expect(createNativeAccountCompaniesReader(JSON.stringify(config),async()=>({status:200,body:{data:{...view,...delta}}}))(h,identity,'h001')).rejects.toMatchObject({code:'NATIVE_ACCOUNT_COMPANIES_RESPONSE_INVALID'});
 await expect(createNativeAccountCompaniesReader(JSON.stringify(config),send)(h,identity,'h002')).rejects.toMatchObject({code:'NATIVE_ACCOUNT_COMPANIES_TARGET_UNAVAILABLE'});expect(send).toHaveBeenCalledTimes(1);
 expect((await createNativeAccountCompaniesReader(JSON.stringify(config),async()=>({status:200,body:{data:{...view,companies:[]}}}))(h,identity)).companies).toEqual([]);
});

it('renders native role choices and current organization paths without the retired Organization SQL reader',async()=>{
 const scope={kind:'holding' as const,holdingId:'h001',holdingSlug:'acme',holdingName:'Synthetic Holding',companyId:'company'};
 const access={scope,sessionId:'synthetic-session',userId:'alpha',expiresAt:Date.now()+60000,name:'Synthetic Alpha',email:'alpha@example.test',assignments:[],requestHeaders:incoming()};
 const choice={id:'branch-grant',version:3,role:'custom_review',companyId:'company',branchId:'native-branch',modules:[],scopeKind:'branch',scopeNodeId:'native-branch',label:'Native role · Native branch',validUntil:Date.now()+60000,branchSlug:'native-branch-path',departmentSlug:null};
 const send=vi.fn(async()=>({status:200,body:{data:{holdingId:'h001',companyId:'company',userId:'alpha',sessionId:'synthetic-session',expiresAt:Date.now()+60000,options:[choice],selection:{assignmentId:'branch-grant',assignmentVersion:3,revision:9,expiresAt:Date.now()+60000},valid:false,branchId:null,departmentId:null,branchSlug:null,departmentSlug:null}}}));
 const prepare=vi.fn(()=>{throw Error('RETIRED_ROLE_OPTIONS_SQL_MUST_NOT_RUN');}),write={prepare};
 vi.resetModules();vi.doMock('@erpx/server-identity_access/public/authorization',()=>({requireAccountAccess:async()=>access}));vi.doMock('@erpx/server-runtime/public/runtime',()=>({holdingResources:async()=>({write})}));vi.doMock('@erpx/native-sdk/field-service-parts',()=>({nativeWorkRolesRequest:send}));
 try{
  vi.stubEnv('ERPX_NATIVE_FIELD_SERVICE_WORKSPACE_CONFIG',JSON.stringify({privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingId:'h001',holdingSlug:'acme'}]}));requestHeaders.mockResolvedValue(incoming());
  const {workRoleOptions}=await import('../../../packages/server-work_projection/src/adapters/work-role-options');const result=await workRoleOptions('acme');
  expect(result.options).toHaveLength(1);expect(result.options[0]).toMatchObject({id:'branch-grant',version:3,label:choice.label});expect(result.options[0]!.href).toContain('/branches/native-branch-path/home');expect(result.selection).toMatchObject({assignment_id:'branch-grant',assignment_version:3,revision:9});expect(result.selected?.id).toBe('branch-grant');expect(prepare).not.toHaveBeenCalled();expect(send).toHaveBeenCalledOnce();
 }finally{vi.unstubAllEnvs();vi.doUnmock('@erpx/server-identity_access/public/authorization');vi.doUnmock('@erpx/server-runtime/public/runtime');vi.doUnmock('@erpx/native-sdk/field-service-parts');vi.resetModules();}
});

it('keeps preference transport bound to the current account and exact Holding without a SQL fallback',async()=>{
 const {createNativeAccountPreferencesClient}=await import('../../../packages/server-work_projection/src/adapters/native-account-preferences');
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingSlug:'acme',holdingId:'h001'}]};
 const value={holdingId:'h001',holdingSlug:'acme',holdingName:'Synthetic Holding',userId:'alpha',sessionId:'synthetic-session',workspace:{workspacePath:'/holdings/acme/home',platformAssignmentId:'retained-platform'},company:{companyId:'company'}};
 const calls:Parameters<typeof import('@erpx/native-sdk/field-service-parts').nativeAccountPreferencesRequest>[0][]=[];
 const send=vi.fn(async(input:typeof calls[number])=>{calls.push(input);return {status:200,body:{data:value}};});
 const client=createNativeAccountPreferencesClient(JSON.stringify(config),send),h=incoming();
 expect(await client(h,'alpha','GET',undefined,'h001')).toEqual(value);expect([...calls[0]!.headers.keys()].sort()).toEqual(['accept','cookie','host']);
 await expect(client(h,'alpha','POST','{"companyId":"company"}',undefined,'acme')).rejects.toMatchObject({code:'ORIGIN_FORBIDDEN'});expect(send).toHaveBeenCalledOnce();
 h.set('origin','https://acme.example.test');h.set('x-erpx-context-company','company');const body='{"companyId":"company","extra":"retained transport input"}';
 await client(h,'alpha','POST',body,undefined,'acme');expect(calls[1]!.body).toBe(body);expect(calls[1]!.headers.get('x-erpx-context-company')).toBe('company');
 for(const delta of [{userId:'beta'},{holdingId:'h002'},{holdingSlug:'northstar'}])await expect(createNativeAccountPreferencesClient(JSON.stringify(config),async()=>({status:200,body:{data:{...value,...delta}}}))(h,'alpha','GET')).rejects.toMatchObject({code:'NATIVE_ACCOUNT_PREFERENCE_RESPONSE_INVALID'});
 await expect(client(h,'alpha','GET',undefined,'h002')).rejects.toMatchObject({code:'NATIVE_ACCOUNT_PREFERENCE_TARGET_UNAVAILABLE'});expect(send).toHaveBeenCalledTimes(2);
});

it('keeps Company selection as native transport and sets a Secure HttpOnly Holding cookie from its acknowledgement',async()=>{
 const choose=vi.fn(async()=>({holdingId:'h001',holdingSlug:'acme',holdingName:'Synthetic Holding',userId:'alpha',sessionId:'synthetic-session',company:{companyId:'company'},workspace:{workspacePath:'/holdings/acme/home',platformAssignmentId:null}}));
 vi.resetModules();vi.doMock('@erpx/server-identity_access/public/authorization',()=>({currentSession:async()=>({session:{user:{id:'alpha'}}})}));vi.doMock('@erpx/server-work_projection/public/workspace-preferences',()=>({selectNativeCompanyPreference:choose}));
 try{
  vi.stubEnv('ERPX_NATIVE_ACCOUNT_SESSION_CONFIG','{}');
  const {POST}=await import('../../../packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/companies/active/route');
  const body='{"companyId":"company","extra":"leave schema decisions to Rust"}',h=incoming();h.set('origin','https://acme.example.test');
  const response=await POST(new Request('https://acme.example.test/api/v1/holdings/acme/companies/active',{method:'POST',headers:h,body}),{params:Promise.resolve({holdingSlug:'acme'})});
  expect(response.status).toBe(200);expect(await response.json()).toEqual({data:{companyId:'company'}});expect(choose).toHaveBeenCalledWith(expect.anything(),'alpha',body,'acme');
  const cookie=response.headers.get('set-cookie');expect(cookie).toContain('erpx_company_h001=company');expect(cookie).toMatch(/; Secure/i);expect(cookie).toMatch(/; HttpOnly/i);expect(cookie).toMatch(/; SameSite=lax/i);expect(cookie).toContain('Path=/');
 }finally{vi.unstubAllEnvs();vi.doUnmock('@erpx/server-identity_access/public/authorization');vi.doUnmock('@erpx/server-work_projection/public/workspace-preferences');vi.resetModules();}
});

it('keeps native SSR Company hints separate from captured Work and rejects incomplete presentation',async()=>{
 const {createNativeWorkRoleTransport}=await import('@erpx/server-identity_access/public/native-work-role-transport');
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingId:'h001',holdingSlug:'acme'}]};
 const state={holdingId:'h001',companyId:'company',sessionId:'synthetic-session',userId:'alpha',expiresAt:Date.now()+60000,holdingName:'Synthetic Holding',companyName:'Synthetic Company',companySlug:'acme',branchName:null,departmentName:null,workspaces:[],context:null,options:[],selection:null,valid:false,branchId:null,departmentId:null,branchSlug:null,departmentSlug:null};
 const send=vi.fn(async(input:Parameters<typeof import('@erpx/native-sdk/field-service-parts').nativeWorkRolesRequest>[0])=>{expect(input.mode).toBe('access');expect(input.headers.get('x-erpx-requested-company')).toBe('company');expect(input.headers.has('x-erpx-context-company')).toBe(false);expect(input.headers.has('x-erpx-context-assignment')).toBe(false);return {status:200,body:{data:state}};});
 const h=incoming();h.set('x-erpx-context-company','forged-company');h.set('x-erpx-context-assignment','forged-role');
 expect(await createNativeWorkRoleTransport(JSON.stringify(config),send)(h,{sessionId:'synthetic-session',userId:'alpha',companyId:'company'},'access')).toEqual(state);
 const frameSend=vi.fn(async(input:Parameters<typeof import('@erpx/native-sdk/field-service-parts').nativeWorkRolesRequest>[0])=>{expect(input.mode).toBe('context');expect(input.headers.get('x-erpx-context-company')).toBe('forged-company');expect(input.headers.get('x-erpx-context-assignment')).toBe('forged-role');return {status:409,body:{error:{code:'WORK_CONTEXT_CHANGED'}}};});
 await expect(createNativeWorkRoleTransport(JSON.stringify(config),frameSend)(h,{sessionId:'synthetic-session',userId:'alpha'},'context')).rejects.toMatchObject({status:409,code:'WORK_CONTEXT_CHANGED'});
 for(const delta of [{valid:true,context:null},{workspaces:[{kind:'department',id:'d',label:'Synthetic department',inherited:false,branchSlug:null,departmentSlug:null}]}])await expect(createNativeWorkRoleTransport(JSON.stringify(config),async()=>({status:200,body:{data:{...state,...delta}}}))(h,{sessionId:'synthetic-session',userId:'alpha'},'context')).rejects.toMatchObject({code:'NATIVE_WORK_ROLE_RESPONSE_INVALID'});
});

it('maps native account/work reads without recomputing business authority or accepting a stale Account',async()=>{
 const {nativeAccountReadModel,nativeWorkReadModel}=await import('../../../packages/server-identity_access/src/adapters/native-account-context');
 const session={session:{id:'synthetic-session',expiresAt:Date.now()+60000},user:{id:'alpha',name:'Synthetic account',email:'alpha@example.test'}};
 const state:import('@erpx/contracts/field-service-reserve').NativeWorkRoleState={holdingId:'h001',companyId:'company',sessionId:'synthetic-session',userId:'alpha',expiresAt:Date.now()+60000,holdingName:'Synthetic Holding',companyName:'Synthetic Company',companySlug:'acme',branchName:null,departmentName:null,workspaces:[{kind:'branch',id:'branch',label:'Synthetic branch',inherited:true,branchSlug:'bangkok',departmentSlug:null}],context:{expiresAt:Date.now()+60000,modules:['field_service'],beta:false,iam:{roleRevision:2,identityVersion:3,permissions:['service.read'],grantablePermissions:[],principalId:'principal',personId:null,personUserIds:['alpha'],hrAppointmentVersion:null,emergencyUntil:null,freshUntil:null}},options:[{id:'selected',version:4,role:'field-engineer',companyId:'company',branchId:null,scopeKind:'holding',scopeNodeId:null,modules:['ignored-draft-module'],label:'Synthetic role',validUntil:Date.now()+60000,branchSlug:null,departmentSlug:null}],selection:{assignmentId:'selected',assignmentVersion:4,revision:5,expiresAt:Date.now()+60000},valid:true,branchId:null,departmentId:null,branchSlug:null,departmentSlug:null};
 const access=nativeAccountReadModel(state,session,incoming(),{kind:'holding',holdingSlug:'acme'});const work=nativeWorkReadModel(state,access);
 expect(work).toMatchObject({authority:{sessionId:'synthetic-session',revision:5,assignmentVersion:4},modules:['field_service'],iam:{permissions:['service.read'],roleRevision:2,identityVersion:3},workspaces:[{href:'/holdings/acme/branches/bangkok/home'}]});expect(work.iam).not.toHaveProperty('hrAppointmentVersion');
 const {createNativeWorkRoleTransport}=await import('@erpx/server-identity_access/public/native-work-role-transport');
 const config={privateBaseUrl:'http://127.0.0.1:43117',timeoutMs:250,holdings:[{origin:'https://acme.example.test',holdingId:'h001',holdingSlug:'acme'}]};
 const zero={...state,context:{...state.context!,iam:{...state.context!.iam,identityVersion:0}}};
 expect((await createNativeWorkRoleTransport(JSON.stringify(config),async()=>({status:200,body:{data:zero}}))(incoming(),{sessionId:session.session.id,userId:session.user.id,companyId:'company'},'context')).context?.iam.identityVersion).toBe(0);

 expect(()=>nativeAccountReadModel(state,{...session,session:{...session.session,id:'stale-account'}},incoming(),{kind:'holding',holdingSlug:'acme'})).toThrow(expect.objectContaining({code:'WORK_CONTEXT_CHANGED'}));
 expect(()=>nativeAccountReadModel(state,session,incoming(),{kind:'branch',holdingSlug:'acme',branchSlug:'bangkok'})).toThrow(expect.objectContaining({code:'NATIVE_WORK_ROLE_RESPONSE_INVALID'}));
 expect(()=>nativeWorkReadModel({...state,valid:false},access)).toThrow(expect.objectContaining({code:'WORK_ROLE_UNAVAILABLE'}));
 expect(()=>nativeWorkReadModel({...state,selection:null},access)).toThrow(expect.objectContaining({code:'WORK_ROLE_REQUIRED'}));
});

it('keeps original Company hint precedence without loading unrelated preferences or silently replacing a withdrawn choice',async()=>{
 const {nativeCompanyHint}=await import('../../../packages/server-identity_access/src/adapters/native-account-context');
 const requested={kind:'holding' as const,holdingSlug:'acme'},memory=vi.fn(async()=> 'remembered'),available=vi.fn(async()=>[{id:'sole-current-company'}]);
 const h=new Headers({'x-erpx-context-company':'context',cookie:'erpx_company_h001=cookie'});
 expect(await nativeCompanyHint(h,'h001',requested,{companyId:'original'},memory,available)).toBe('original');expect(memory).not.toHaveBeenCalled();
 expect(await nativeCompanyHint(h,'h001',requested,{},memory,available)).toBe('context');h.delete('x-erpx-context-company');expect(await nativeCompanyHint(h,'h001',requested,{},memory,available)).toBe('cookie');
 h.set('x-erpx-front-company','front');expect(await nativeCompanyHint(h,'h001',requested,{},memory,available)).toBeUndefined();expect(memory).not.toHaveBeenCalled();expect(available).not.toHaveBeenCalled();
 h.delete('x-erpx-front-company');h.set('cookie','erpx_company_h001=withdrawn');expect(await nativeCompanyHint(h,'h001',requested,{},memory,available)).toBe('withdrawn');expect(memory).not.toHaveBeenCalled();expect(available).not.toHaveBeenCalled();
 h.delete('cookie');expect(await nativeCompanyHint(h,'h001',requested,{},memory,available)).toBe('remembered');expect(memory).toHaveBeenCalledOnce();expect(available).not.toHaveBeenCalled();
 expect(await nativeCompanyHint(h,'h001',requested,{},async()=>undefined,available)).toBe('sole-current-company');expect(await nativeCompanyHint(h,'h001',requested,{},async()=>undefined,async()=>[{id:'one'},{id:'two'}])).toBeUndefined();
 expect(await nativeCompanyHint(h,'h001',{kind:'branch',holdingSlug:'acme',branchSlug:'branch'},{companyId:'original'},memory,available)).toBe('original');
});

it('proxies native authentication and SSO with separate public authority, opaque body, cookies and unfollowed redirects',async()=>{
 const {createNativeAccountHttpTransport}=await import('@erpx/native-sdk/session');
 const cookies=['__Host-erpx-product=synthetic; Path=/; Secure; HttpOnly; SameSite=Lax','__Host-erpx-sso=; Path=/; Secure; HttpOnly; Max-Age=0'];
 const send=vi.fn(async(url:URL,init:RequestInit)=>{
  expect(url.origin).toBe(configuration.privateBaseUrl);expect(new Headers(init.headers).get('host')).toBe('acme.example.test');expect(new Headers(init.headers).has('x-forwarded-host')).toBe(false);expect(init.redirect).toBe('manual');
  const headers=new Headers({location:'https://account.example.test/login?sso=synthetic','content-type':'application/json'});for(const c of cookies)headers.append('set-cookie',c);return new Response(null,{status:303,headers});
 });
 const response=await createNativeAccountHttpTransport(configuration,send)(new Request('http://127.0.0.1:43118/sso/start?next=%2Fhome',{headers:{host:'acme.example.test',cookie:'__Host-erpx-product=synthetic','x-forwarded-host':'forged.example.test'}}));
 expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://account.example.test/login?sso=synthetic');expect(response.headers.getSetCookie()).toEqual(cookies);expect(response.headers.get('cache-control')).toBe('private, no-store');expect(send).toHaveBeenCalledOnce();
 const body='{"email":"synthetic@example.test","password":"synthetic-test-only","revision":3}';
 const post=vi.fn(async(_url:URL,init:RequestInit)=>{expect(init.method).toBe('POST');expect(init.body).toBe(body);expect(new Headers(init.headers).get('origin')).toBe(configuration.accountOrigin);return Response.json({error:{code:'INVALID_CREDENTIALS'}},{status:401});});
 const reply=await createNativeAccountHttpTransport(configuration,post)(new Request(configuration.accountOrigin+'/api/auth/sign-in/email',{method:'POST',headers:{host:'account.example.test',origin:configuration.accountOrigin,'content-type':'application/json'},body}));expect(reply.status).toBe(401);expect(await reply.json()).toEqual({error:{code:'INVALID_CREDENTIALS'}});
});
it('rejects unregistered authentication authority/origin and oversized bodies before private IO with no fallback',async()=>{
 const {createNativeAccountHttpTransport}=await import('@erpx/native-sdk/session');const send=vi.fn(async()=>new Response(null,{status:200})),run=createNativeAccountHttpTransport(configuration,send);
 await expect(run(new Request('https://retired.example.test/api/auth/get-session',{headers:{host:'retired.example.test'}}))).rejects.toMatchObject({code:'NATIVE_ACCOUNT_AUTHORITY_INVALID'});
 for(const origin of [undefined,'https://foreign.example.test'])await expect(run(new Request(configuration.accountOrigin+'/api/auth/sign-in/email',{method:'POST',headers:{host:'account.example.test',...(origin?{origin}:{})},body:'{}'}))).rejects.toMatchObject({code:'NATIVE_ACCOUNT_ORIGIN_FORBIDDEN'});
 await expect(run(new Request(configuration.accountOrigin+'/api/auth/sign-in/email',{method:'POST',headers:{host:'account.example.test',origin:configuration.accountOrigin},body:'x'.repeat(8193)}))).rejects.toMatchObject({code:'NATIVE_ACCOUNT_PAYLOAD_LIMIT'});expect(send).not.toHaveBeenCalled();
 expect((await run(new Request(configuration.accountOrigin+'/api/v1/arbitrary-business-writer',{headers:{host:'account.example.test'}}))).status).toBe(404);expect(send).not.toHaveBeenCalled();
});

it('preserves native drawer CORS preflight and DELETE input without inventing Account decisions',async()=>{
 const {createNativeAccountHttpTransport}=await import('@erpx/native-sdk/session');
 const send=vi.fn(async(url:URL,init:RequestInit)=>{expect(url.pathname).toBe('/api/v1/account/device-accounts');const h=new Headers(init.headers);expect(h.get('host')).toBe('account.example.test');expect(h.get('origin')).toBe(configuration.holdingOrigins[0]);
  if(init.method==='OPTIONS'){expect(h.get('access-control-request-method')).toBe('DELETE');expect(h.get('access-control-request-headers')).toBe('content-type');return new Response(null,{status:204,headers:{'access-control-allow-origin':configuration.holdingOrigins[0],'access-control-allow-credentials':'true'}});}
  expect(init.method).toBe('DELETE');expect(init.body).toBe('{"handle":"synthetic-handle","revision":7}');return Response.json({error:{code:'DEVICE_ACCOUNTS_CHANGED'}},{status:409});
 });
 const run=createNativeAccountHttpTransport(configuration,send),h={host:'account.example.test',origin:configuration.holdingOrigins[0]};
 const preflight=await run(new Request(configuration.accountOrigin+'/api/v1/account/device-accounts',{method:'OPTIONS',headers:{...h,'access-control-request-method':'DELETE','access-control-request-headers':'content-type'}}));expect(preflight.status).toBe(204);expect(preflight.headers.get('access-control-allow-origin')).toBe(configuration.holdingOrigins[0]);expect(preflight.headers.get('access-control-allow-credentials')).toBe('true');
 const removed=await run(new Request(configuration.accountOrigin+'/api/v1/account/device-accounts',{method:'DELETE',headers:{...h,'content-type':'application/json'},body:'{"handle":"synthetic-handle","revision":7}'}));expect(removed.status).toBe(409);expect(await removed.json()).toEqual({error:{code:'DEVICE_ACCOUNTS_CHANGED'}});
});

it('uses the real private HTTP transport for an opaque POST and preserves two auth cookies without following its redirect',async()=>{
 const {createServer}=await import('node:http');const {createNativeAccountHttpTransport}=await import('@erpx/native-sdk/session');let calls=0,body='',authority:string|undefined;
 const server=createServer((request,response)=>{calls++;authority=request.headers.host;expect(request.method).toBe('POST');expect(request.url).toBe('/api/auth/sign-in/email');expect(request.headers.origin).toBe(configuration.accountOrigin);expect(request.headers['x-forwarded-host']).toBeUndefined();request.setEncoding('utf8');request.on('data',chunk=>{body+=chunk;});request.on('end',()=>{response.writeHead(303,{'location':configuration.accountOrigin+'/login?synthetic=1','set-cookie':['__Host-erpx-device=synthetic-device; Path=/; Secure; HttpOnly','__Secure-erpx-account.session_token=synthetic-session; Path=/; Secure; HttpOnly']});response.end();});});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 try{
  const address=server.address();if(!address||typeof address==='string')throw Error('Expected a private TCP test listener');
  const response=await createNativeAccountHttpTransport({...configuration,privateBaseUrl:`http://127.0.0.1:${address.port}`})(new Request(configuration.accountOrigin+'/api/auth/sign-in/email',{method:'POST',headers:{host:'account.example.test',origin:configuration.accountOrigin,'content-type':'application/json','x-forwarded-host':'forged.example.test'},body:'{"email":"synthetic@example.test","password":"synthetic-only"}'}));
  expect(response.status).toBe(303);expect(response.headers.getSetCookie()).toHaveLength(2);expect(response.headers.get('location')).toBe(configuration.accountOrigin+'/login?synthetic=1');expect(calls).toBe(1);expect(authority).toBe('account.example.test');expect(body).toBe('{"email":"synthetic@example.test","password":"synthetic-only"}');
 }finally{server.closeAllConnections();await new Promise<void>((resolve,reject)=>server.close(error=>error?reject(error):resolve()));}
});

it('forwards only native Account MFA enrollment/verification while preserving opaque input',async()=>{
 const {createNativeAccountHttpTransport}=await import('@erpx/native-sdk/session');
 const send=vi.fn(async(url:URL,init:RequestInit)=>{expect(url.pathname).toMatch(/^\/api\/v1\/account\/iam\/mfa\/(enroll|verify)$/);expect(new Headers(init.headers).get('host')).toBe('acme.example.test');expect(init.body).toBe('{"code":"synthetic"}');return Response.json({error:{code:'MFA_INVALID'}},{status:422});});
 const run=createNativeAccountHttpTransport(configuration,send);
 for(const action of ['enroll','verify']){const reply=await run(new Request(configuration.holdingOrigins[0]+'/api/v1/account/iam/mfa/'+action,{method:'POST',headers:{host:'acme.example.test',origin:configuration.holdingOrigins[0],'content-type':'application/json'},body:'{"code":"synthetic"}'}));expect(reply.status).toBe(422);}
 expect(send).toHaveBeenCalledTimes(2);
 const unknown=await run(new Request(configuration.holdingOrigins[0]+'/api/v1/account/iam/mfa/reset',{method:'POST',headers:{host:'acme.example.test',origin:configuration.holdingOrigins[0]},body:'{}'}));expect(unknown.status).toBe(404);expect(send).toHaveBeenCalledTimes(2);
});
