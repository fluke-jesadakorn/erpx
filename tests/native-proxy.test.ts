import { afterEach,beforeEach,it,expect,vi } from 'vitest';
import { NextRequest } from 'next/server';
import { proxy } from '../src/proxy';
import { marketingConsoleUrl } from '../src/lib/erpx-hosts';
afterEach(()=>vi.unstubAllEnvs());
beforeEach(()=>vi.stubEnv('ERPX_MARKETING_ORIGIN',''));
it('keeps branch rewrites on the loopback origin and replaces caller scope headers',()=>{
 vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 const request=new NextRequest('http://127.0.0.1:3101/api/v1/holdings/acme/branches/bangkok/people/people/person?x=1',
  {headers:{'x-erpx-workspace-kind':'holding','x-erpx-department-slug':'forged'}});
 const response=proxy(request);
 expect(response.headers.get('x-middleware-rewrite')).toBe('http://127.0.0.1:3101/api/v1/holdings/acme/people/people/person?x=1');
 expect(response.headers.get('x-middleware-request-x-erpx-workspace-kind')).toBe('branch');
 expect(response.headers.get('x-middleware-request-x-erpx-branch-slug')).toBe('bangkok');
 expect(response.headers.get('x-middleware-request-x-erpx-department-slug')).toBeNull();
});
it('keeps user query parameters in recovery destinations and excludes the internal RSC request key',()=>{
 const request=new NextRequest('http://127.0.0.1:3100/holdings/acme/finance/invoices?new=1&tag=review&tag=draft&_rsc=internal',
  {headers:{'x-erpx-page-path':'/holdings/forged/home'}});
 const response=proxy(request);
 expect(response.headers.get('x-middleware-request-x-erpx-page-path')).toBe('/holdings/acme/finance/invoices?new=1&tag=review&tag=draft');
});
it('rejects foreign hosts, redirects HTTP and sets secure workspace cookies in production',()=>{
 vi.stubEnv('NODE_ENV','production');vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://erpx.jesadakorn.com');
 expect(proxy(new NextRequest('http://127.0.0.1:3101/login',{headers:{host:'evil.example'}})).status).toBe(421);
 const response=proxy(new NextRequest('http://127.0.0.1:3101/login',{headers:{host:'erpx.jesadakorn.com','x-forwarded-proto':'http'}}));
 expect(response.status).toBe(308);expect(response.headers.get('location')).toBe('https://erpx.jesadakorn.com/login');
 const workspace=proxy(new NextRequest('http://127.0.0.1:3101/holdings/acme/home',{headers:{host:'erpx.jesadakorn.com','x-forwarded-proto':'https'}}));
 expect(workspace.cookies.get('erpx_last_workspace')).toMatchObject({secure:true,httpOnly:true});
 expect(workspace.cookies.get('erpx_last_workspace')?.domain).toBeUndefined();
});
it('keeps PMS and POS hosts inside their own Front routes',()=>{
 vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 vi.stubEnv('NODE_ENV','production');
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://erpx.jesadakorn.com');
 vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','https://stage-pms.jesadakorn.com');
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','https://stage-pos.jesadakorn.com');
 const request=(host:string,route:string)=>proxy(new NextRequest('https://127.0.0.1:3103'+route,{headers:{host,'x-forwarded-proto':'https'}}));
 expect(request('unknown.jesadakorn.com','/api/health').status).toBe(421);
 expect(request('127.0.0.1:3103','/api/health').status).toBe(200);
 expect(proxy(new NextRequest('http://127.0.0.1:3103/api/health',{headers:{host:'127.0.0.1:3103','x-forwarded-proto':'http'}})).status).toBe(200);
 expect(request('127.0.0.1:3103','/holdings/acme').status).toBe(421);
 expect(request('stage-pms.jesadakorn.com','/holdings/acme').status).toBe(404);
 expect(request('stage-pms.jesadakorn.com','/api/v1/front/acme/acme-co/pos').status).toBe(404);
 expect(request('stage-pos.jesadakorn.com','/api/v1/front/acme/acme-co/pms').status).toBe(404);
 expect(request('stage-pos.jesadakorn.com','/api/v1/front-access/quick-access').status).toBe(200);
 expect(request('stage-pms.jesadakorn.com','/api/v1/front-access/quick-access').status).toBe(200);
 expect(request('stage-pos.jesadakorn.com','/erpx-push-sw.js').status).toBe(200);
 expect(request('stage-pms.jesadakorn.com','/erpx-push-sw.js').status).toBe(200);
 expect(request('stage-pos.jesadakorn.com','/api/v1/holdings/acme/branches/bangkok/push').headers.get('x-middleware-rewrite')).toBe('https://127.0.0.1:3103/api/v1/holdings/acme/push');
 expect(request('stage-pms.jesadakorn.com','/api/v1/holdings/acme/push').status).toBe(200);
 expect(request('stage-pos.jesadakorn.com','/api/v1/holdings/acme/branches/bangkok/finance').status).toBe(404);
 expect(request('stage-pms.jesadakorn.com','/api/v1/holdings/acme/branches/bangkok/push/other').status).toBe(404);
 expect(request('erpx.jesadakorn.com','/api/v1/front-access/quick-access').status).toBe(404);
 expect(request('stage-pos.jesadakorn.com','/api/v1/front-access/other').status).toBe(404);
 const roleCheck=request('stage-pms.jesadakorn.com','/api/v1/front/acme/acme-co/pms/roles');
 expect(roleCheck.status).toBe(200);
 expect(roleCheck.headers.get('x-middleware-request-x-erpx-front-company')).toBe('acme-co');
 expect(request('stage-pms.jesadakorn.com','/api/auth/sign-up/email').status).toBe(404);
 expect(request('erpx.jesadakorn.com','/api/v1/front/acme/acme-co/pms').status).toBe(404);
 expect(request('stage-pms.jesadakorn.com','/acme/acme-co/holding/dashboard').headers.get('x-middleware-rewrite')).toBe('https://127.0.0.1:3103/front/acme/acme-co/pms/holding/dashboard');
 expect(request('erpx.jesadakorn.com','/front/acme/acme-co/pms/holding').headers.get('location')).toBe('https://stage-pms.jesadakorn.com/acme/acme-co/holding');
 expect(request('stage-pms.jesadakorn.com','/front/acme/acme-co/pos/holding').status).toBe(404);
});
it('serves marketing only on the public host and sends old page links to the console',()=>{
 vi.stubEnv('NODE_ENV','production');
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_MARKETING_ORIGIN','https://dev-erpx.jesadakorn.com');
 const request=(host:string,path:string,method='GET')=>proxy(new NextRequest('https://127.0.0.1:3100'+path,{method,headers:{host,'x-forwarded-proto':'https'}}));
 const home=request('dev-erpx.jesadakorn.com','/');
 expect(home.headers.get('x-middleware-rewrite')).toBe('https://localhost:3100/marketing');
 expect(home.headers.get('x-robots-tag')).toBe('noindex, nofollow');
 const deep=request('dev-erpx.jesadakorn.com','/book/acme?arrival=2026-10-01');
 expect(deep.status).toBe(308);
 expect(deep.headers.get('location')).toBe('https://console-dev-erpx.jesadakorn.com/book/acme?arrival=2026-10-01');
 expect(request('dev-erpx.jesadakorn.com','/api/auth/get-session').status).toBe(404);
 expect(request('dev-erpx.jesadakorn.com','/_next/static/chunks/site.js').status).toBe(200);
 expect(request('dev-erpx.jesadakorn.com','/api/v1/stripe/webhook').status).toBe(404);
 expect(request('dev-erpx.jesadakorn.com','/api/v1/stripe/webhook','POST').status).toBe(404);
 expect(request('dev-erpx.jesadakorn.com','/api/pos/stripe/webhook','POST').status).toBe(404);
 expect(request('console-dev-erpx.jesadakorn.com','/marketing').status).toBe(404);
 expect(request('console-dev-erpx.jesadakorn.com','/login').status).toBe(200);
 expect(request('dev-erpx.jesadakorn.com','/robots.txt').status).toBe(200);
 expect(request('dev-erpx.jesadakorn.com','/sitemap.xml').status).toBe(404);
});
it('indexes only the staging marketing homepage',()=>{
 vi.stubEnv('NODE_ENV','production');
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_MARKETING_ORIGIN','https://erpx.jesadakorn.com');
 const request=(host:string,path:string)=>proxy(new NextRequest('https://127.0.0.1:3103'+path,{headers:{host,'x-forwarded-proto':'https'}}));
 expect(request('erpx.jesadakorn.com','/').headers.get('x-robots-tag')).toBeNull();
 expect(request('erpx.jesadakorn.com','/sitemap.xml').status).toBe(200);
 expect(request('console-erpx.jesadakorn.com','/sitemap.xml').status).toBe(404);
 expect(request('erpx.jesadakorn.com','/legal-portal/acme?token=sample').headers.get('location')).toBe('https://console-erpx.jesadakorn.com/legal-portal/acme?token=sample');
});
it('keeps unusual old paths on the configured console host',()=>{
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-erpx.jesadakorn.com');
 expect(marketingConsoleUrl('//elsewhere.example/path','?ref=old').href)
  .toBe('https://console-erpx.jesadakorn.com//elsewhere.example/path?ref=old');
});
it('serves the shared Thai font assets and read-only signed-in time preference on Front without exposing account commands',()=>{
 vi.stubEnv('NODE_ENV','production');
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','https://pos-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','https://pms-dev-erpx.jesadakorn.com');
 const request=(host:string,path:string,method='GET')=>proxy(new NextRequest('https://127.0.0.1:3104'+path,{method,headers:{host,'x-forwarded-proto':'https'}}));
 for(const host of ['pos-dev-erpx.jesadakorn.com','pms-dev-erpx.jesadakorn.com']){
  for(const font of ['NotoSansThai.ttf','NotoSansThai.woff2','NotoSansThaiOther.woff2'])expect(request(host,'/fonts/'+font).status).toBe(200);
  expect(request(host,'/api/v1/account/time-preference').status).toBe(200);
  expect(request(host,'/api/v1/account/time-preference','PATCH').status).toBe(404);
  expect(request(host,'/api/v1/account/security').status).toBe(404);
  expect(request(host,'/fonts/private.woff2').status).toBe(404);
 }
});

it('returns an early 404 for retired UI destinations across Platform and every Holding scope without redirects or rewrites',()=>{
 vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 const scopes=['/platform','/holdings/acme','/holdings/acme/branches/bangkok','/holdings/acme/departments/finance','/holdings/acme/branches/bangkok/departments/finance'];
 const destinations=['modules','modules/finance/ledger','records/finance/new','records/finance/record-1','preview/records/finance/sample-record'];
 for(const method of ['GET','HEAD'])for(const scope of scopes)for(const destination of destinations)for(const suffix of ['','/','?q=review&_rsc=stream']){
  const path=`${scope}/${destination}${suffix}`;
  const response=proxy(new NextRequest('http://127.0.0.1:3102'+path,{method}));
  expect(response.status,`${method} ${path}`).toBe(404);
  expect(response.headers.get('location')).toBeNull();
  expect(response.headers.get('x-middleware-rewrite')).toBeNull();
  expect(response.headers.get('x-middleware-next')).toBeNull();
  expect(response.cookies.get('erpx_last_workspace')).toBeUndefined();
 }
});
it('preserves live routes, prefix lookalikes and records APIs while rejecting only retired UI destination segments',()=>{
 vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 const destinations=['home','finance','projects','modules-extra','records-extra','preview/requests','preview/records-extra'];
 const scopes=[{base:'/holdings/acme',kind:'holding'},{base:'/holdings/acme/branches/bangkok',kind:'branch'},{base:'/holdings/acme/departments/finance',kind:'department'},{base:'/holdings/acme/branches/bangkok/departments/finance',kind:'department'}];
 for(const method of ['GET','HEAD']){
  for(const destination of destinations){
   const platform=proxy(new NextRequest(`http://127.0.0.1:3102/platform/${destination}?q=review`,{method}));
   expect(platform.status).toBe(200);
   expect(platform.headers.get('location')).toBeNull();
   expect(platform.headers.get('x-middleware-next')).toBe('1');
   expect(platform.headers.get('x-middleware-request-x-erpx-page-path')).toBe(`/platform/${destination}?q=review`);
   expect(platform.cookies.get('erpx_last_workspace')?.value).toBe(method==='GET'?`/platform/${destination}`:undefined);
  }
  for(const scope of scopes)for(const api of [false,true])for(const destination of api?['records/finance/new','modules/finance/ledger','preview/records/finance/sample-record']:destinations){
   const path=`${api?'/api/v1':''}${scope.base}/${destination}`;
   const response=proxy(new NextRequest(`http://127.0.0.1:3102${path}?q=review`,{method,headers:{'x-erpx-workspace-kind':'forged','x-erpx-department-slug':'forged'}}));
   expect(response.status,`${method} ${path}`).toBe(200);
   expect(response.headers.get('location')).toBeNull();
   expect(response.headers.get('x-middleware-request-x-erpx-workspace-kind')).toBe(scope.kind);
   expect(response.headers.get('x-middleware-request-x-erpx-api')).toBe(String(api));
   expect(response.headers.get('x-middleware-request-x-erpx-method')).toBe(method);
   expect(response.headers.get('x-middleware-request-x-erpx-destination')).toBe(destination);
   expect(response.headers.get('x-middleware-request-x-erpx-page-path')).toBe(`${path}?q=review`);
   expect(response.headers.get('x-middleware-request-x-erpx-branch-slug')).toBe(scope.base.includes('/branches/')?'bangkok':null);
   expect(response.headers.get('x-middleware-request-x-erpx-department-slug')).toBe(scope.kind==='department'?'finance':null);
   expect(response.headers.get('x-middleware-rewrite')).toBe(scope.kind==='holding'?null:`http://127.0.0.1:3102${api?'/api/v1':''}/holdings/acme/${destination}?q=review`);
   expect(response.headers.get('x-middleware-next')).toBe(scope.kind==='holding'?'1':null);
   expect(response.cookies.get('erpx_last_workspace')?.value).toBe(!api&&method==='GET'?path:undefined);
  }
  const platformApi=proxy(new NextRequest('http://127.0.0.1:3102/api/v1/platform/records/finance/new',{method}));
  expect(platformApi.status).toBe(200);
  expect(platformApi.headers.get('x-middleware-next')).toBe('1');
  expect(platformApi.headers.get('x-middleware-request-x-erpx-method')).toBe(method);
 }
});
