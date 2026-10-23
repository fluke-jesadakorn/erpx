import {applicationRequest} from '@erpx/server-runtime/public/application-request';
import {beforeEach,expect,it,vi} from 'vitest';
vi.mock('server-only',()=>({}));
const mocks=vi.hoisted(()=>({session:vi.fn(),enabled:true,host:'account-dev-erpx.jesadakorn.com'}));
vi.mock("@erpx/server-identity_access/adapters/authorization",()=>({currentSession:mocks.session}));
vi.mock("@erpx/feature-runtime/lib/product-hosts",()=>({configuredProductOrigin:()=>mocks.enabled?'https://account-dev-erpx.jesadakorn.com':null,productHostForHost:(host:string|null)=>mocks.enabled&&host==='account-dev-erpx.jesadakorn.com'?'account':null}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:mocks.host})}));
vi.mock("@erpx/server-identity_access/public/directory-lookup",()=>({routedConnection:vi.fn()}));
vi.mock("@erpx/server-runtime/public/presentation-configuration",()=>({publicBetaEnabled:async()=>false}));
vi.mock("@erpx/feature-identity_access/lib/auth-client",()=>({authClient:{signIn:{email:vi.fn()}}}));
import {isValidElement,type ReactNode,type ReactElement} from 'react';
import {accountWorkspaceTarget,accountWorkspaceLoginReturn} from '@erpx/server-identity_access/public/rules/account-workspace';
import {accountWorkspaceRequest} from '@erpx/server-identity_access/public/account-workspace';
import {ApiError} from '@erpx/server-runtime/public/http';
const origin='https://account-dev-erpx.jesadakorn.com';
const request=(next='/my-access?tag=one&tag=two#roles',host='account-dev-erpx.jesadakorn.com')=>new Request('https://'+host+'/account/launch?'+new URLSearchParams({next}),{headers:{host}});
beforeEach(()=>{vi.clearAllMocks();mocks.enabled=true;mocks.host='account-dev-erpx.jesadakorn.com';mocks.session.mockResolvedValue({session:{user:{id:'platform-only'},session:{id:'current-central-session'}}});});
it('admits only three exact self paths and removes callback credentials while preserving ordinary repeated query and fragment',()=>{
 for(const path of ['/my-access','/security','/my-profile'])expect(accountWorkspaceTarget(path+'?code=private&state=secret&token=secret&session_token=secret&_rsc=private&tag=one&tag=two#details')).toBe(path+'?tag=one&tag=two#details');
 for(const value of ['/','/my-profile/other','/my-profile/','/holdings/acme/people-organization/profiles/user/overview','/api/v1/account/profile','/sso/callback','/account/launch?next=/security','https://evil.test/my-access','//evil.test/my-access','/x/../my-access','/%6dy-access','/my-access%2fother','/my-access?x=%00','/security?x=%5c','/my-access?bad=%zz'])expect(accountWorkspaceTarget(value)).toBeNull();
});
it('repeats actual central account authentication without requiring Company selection or creating role/session authority',async()=>{
 const response=await accountWorkspaceRequest(request('/security?tag=one&tag=two#sessions'));expect(response.status).toBe(303);expect(response.headers.get('location')).toBe(origin+'/security?tag=one&tag=two#sessions');expect(response.headers.get('cache-control')).toBe('private, no-store');expect(response.headers.get('set-cookie')).toBeNull();expect(mocks.session).toHaveBeenCalledTimes(1);
 for(const protocol of ['http','https']){
  const transport=new Request(protocol+'://127.0.0.1:3250/account/launch?'+new URLSearchParams({next:'/my-profile?tag=one&tag=two#profile'}),{headers:{host:'account-dev-erpx.jesadakorn.com',origin,cookie:'synthetic-cookie=unchanged','x-forwarded-host':'evil.test'}});
  const normalized=applicationRequest(transport,origin,'3250');
  expect(new URL(normalized.url).origin).toBe(origin);expect([...normalized.headers]).toEqual([...transport.headers]);
  const redirected=await accountWorkspaceRequest(normalized);expect(redirected.status).toBe(303);expect(redirected.headers.get('location')).toBe(origin+'/my-profile?tag=one&tag=two#profile');expect(redirected.headers.get('set-cookie')).toBeNull();
 }
 expect(mocks.session).toHaveBeenCalledTimes(3);
});
it('preserves the exact clean self target through normal login recovery without an Account child SSO transaction',async()=>{
 for(const error of [new Error('UNAUTHENTICATED'),new ApiError(401,'SESSION_REVOKED','Sign in again')]){mocks.session.mockRejectedValue(error);const response=await accountWorkspaceRequest(request('/my-profile?token=secret&tag=one&tag=two#profile')),login=new URL(response.headers.get('location')!);expect(response.status).toBe(303);expect(login.origin).toBe(origin);expect(login.pathname).toBe('/login');const bridge=new URL(login.searchParams.get('next')!,origin);expect(bridge.pathname).toBe('/account/launch');expect(bridge.searchParams.get('next')).toBe('/my-profile?tag=one&tag=two#profile');expect(response.headers.get('set-cookie')).toBeNull();}
});
it('rejects foreign or disabled hosts, duplicate or unknown returns, and mutation requests before authentication',async()=>{
 const internal=new Request('http://127.0.0.1:3250/account/launch?next=/my-access',{headers:{host:'account-dev-erpx.jesadakorn.com'}});
 for(const port of [undefined,'3251','3100','3250.0','03250','3250 '])expect(applicationRequest(internal,origin,port)).toBe(internal);
 expect(applicationRequest(internal,null,'3250')).toBe(internal);
 for(const url of ['https://evil.test/account/launch?next=/my-access','http://localhost:3250/account/launch?next=/my-access','http://127.0.0.1:3251/account/launch?next=/my-access']){
  const foreign=new Request(url,{headers:internal.headers});expect(applicationRequest(foreign,origin,'3250')).toBe(foreign);expect((await accountWorkspaceRequest(foreign)).status).toBe(404);
 }
 const wrongHost=new Request(internal,{headers:{host:'evil.test','x-forwarded-host':'account-dev-erpx.jesadakorn.com'}});expect(applicationRequest(wrongHost,origin,'3250')).toBe(wrongHost);
 const post=new Request(internal,{method:'POST',body:'never-forward'});expect(applicationRequest(post,origin,'3250')).toBe(post);

 expect((await accountWorkspaceRequest(request('/my-access','admin-dev-erpx.jesadakorn.com'))).status).toBe(404);expect((await accountWorkspaceRequest(new Request('https://evil.test/account/launch?next=/my-access',{headers:{host:'account-dev-erpx.jesadakorn.com'}}))).status).toBe(404);
 for(const search of ['next=/my-access&next=/security','next=/my-access&token=secret','next=/api/v1/account/profile','next=//evil.test/security'])expect((await accountWorkspaceRequest(new Request(origin+'/account/launch?'+search,{headers:{host:'account-dev-erpx.jesadakorn.com'}}))).status).toBe(422);
 const mutation=await accountWorkspaceRequest(new Request(request(),{method:'POST',body:'next=/my-access'}));expect(mutation.status).toBe(405);expect(mutation.headers.get('allow')).toBe('GET');mocks.enabled=false;expect((await accountWorkspaceRequest(request())).status).toBe(404);expect(mocks.session).not.toHaveBeenCalled();
});
it('does not turn an authority or infrastructure failure into a successful self redirect or login recovery',async()=>{const error=new ApiError(403,'FORBIDDEN','Denied');mocks.session.mockRejectedValue(error);await expect(accountWorkspaceRequest(request())).rejects.toBe(error);});
it('admits only a closed sanitized bridge to Account login and preserves its fragment through repeated login validation',()=>{
 const next=accountWorkspaceLoginReturn('/security?code=secret&tag=one&tag=two#sessions');expect(next).toBe('/account/launch?next=%2Fsecurity%3Ftag%3Done%26tag%3Dtwo%23sessions');expect(accountWorkspaceLoginReturn(next)).toBe(next);
 for(const value of ['/holdings/acme/work','/platform/users','https://evil.test/security','/account/launch?next=/security&next=/my-access','/account/launch?next=/api/v1/account/profile','/account/launch?next=/security#override','/account/launch?next=/security&code=private'])expect(accountWorkspaceLoginReturn(value)).toBeNull();
});
it('passes the exact Account bridge through the actual login Server page while preserving opaque SSO and Console legacy returns',async()=>{
 const {default:LoginPage}=await import('@erpx/workplace/routes/login/page');
 function find(node:ReactNode):ReactElement<{next?:string}>|undefined{if(Array.isArray(node)){for(const child of node){const result=find(child);if(result)return result;}return;}if(!isValidElement<{children?:ReactNode}>(node))return;if(typeof node.type==='function'&&node.type.name==='LoginForm')return node as ReactElement<{next?:string}>;return find(node.props.children);}
 const target='/my-profile?tag=one&tag=two#profile',bridge=accountWorkspaceLoginReturn(target)!;expect(find(await LoginPage({searchParams:Promise.resolve({next:bridge})}))!.props.next).toBe(bridge);
 const transaction={sso:'a'.repeat(8)+'-'+ 'b'.repeat(4)+'-'+ 'c'.repeat(4)+'-'+ 'd'.repeat(4)+'-'+ 'e'.repeat(12),state:'s'.repeat(43),challenge:'c'.repeat(43)};expect(find(await LoginPage({searchParams:Promise.resolve({...transaction,next:bridge})}))!.props.next).toBe('/sso/authorize?'+new URLSearchParams({transaction:transaction.sso,state:transaction.state,challenge:transaction.challenge}));
 mocks.host='console-dev-erpx.jesadakorn.com';expect(find(await LoginPage({searchParams:Promise.resolve({next:'/holdings/acme/work?status=waiting#draft'})}))!.props.next).toBe('/holdings/acme/work?status=waiting#draft');expect(find(await LoginPage({searchParams:Promise.resolve({next:bridge})}))!.props.next).toBeUndefined();
});
