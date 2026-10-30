import { afterEach,beforeEach,it,expect,vi } from 'vitest';
import { proxy as accountProxy } from "../../../apps/account/src/proxy";
import { appProxy } from "../../../scripts/local/app-proxy";
import { NextRequest } from 'next/server';
import { proxy } from "@erpx/workplace/proxy";
import { marketingConsoleUrl } from "@erpx/feature-runtime/lib/erpx-hosts";
import { productIds, sharedHostKeys } from "@erpx/server-commercial/public/product-registry";
import { applicationUrl, applicationOrigin, adminCompanyHandoff, accountRecoveryDeliveryUrl } from "@erpx/feature-runtime/lib/application-origins";
afterEach(()=>vi.unstubAllEnvs());
beforeEach(()=>{
 vi.stubEnv('ERPX_MARKETING_ORIGIN','');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','disabled');
 vi.stubEnv('ERPX_CONSOLE_RETIRED','disabled');
});
it('temporarily preserves scoped Support GET aliases on the same development surface without redirecting mutations',()=>{
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 const query='?q=printer&tag=one&tag=two&return=view%3Dactions';
 const bases=['/holdings/acme','/holdings/acme/branches/bangkok','/holdings/acme/departments/finance','/holdings/acme/branches/bangkok/departments/finance'];
 for(const base of bases)for(const method of ['GET','HEAD']){
  const response=proxy(new NextRequest('http://127.0.0.1:3100'+base+'/support'+query,{method,headers:{host:'console-dev-erpx.jesadakorn.com'}}));
  expect(response.status).toBe(307);
  expect(response.headers.get('location')).toBe('https://console-dev-erpx.jesadakorn.com'+base+'/customers-sales/customers/service/cases'+query);
  expect(response.headers.get('cache-control')).toBe('private, no-store');
 }
 for(const method of ['POST','PUT','PATCH','DELETE']){
  const response=proxy(new NextRequest('http://127.0.0.1:3100/holdings/acme/support'+query,{method,headers:{host:'console-dev-erpx.jesadakorn.com'}}));
  expect(response.headers.get('location')).toBeNull();
 }
 const api=proxy(new NextRequest('http://127.0.0.1:3100/api/v1/holdings/acme/support'+query,{headers:{host:'console-dev-erpx.jesadakorn.com'}}));
 expect(api.headers.get('location')).toBeNull();
});
it('temporarily redirects only the enabled legacy Console entry to Apps, preserving queries and all owner endpoints',()=>{
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_ENVIRONMENT','local');
 vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');
 vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 const request=(path:string,method='GET',host='console-dev-erpx.jesadakorn.com')=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{method,headers:{host}}));
 for(const method of ['GET','HEAD']){
  const response=request('/?ref=bookmark&tag=one&tag=two',method);
  expect(response.status).toBe(307);
  expect(response.headers.get('location')).toBe('https://apps-dev-erpx.jesadakorn.com/?ref=bookmark&tag=one&tag=two');
  expect(response.headers.get('cache-control')).toBe('private, no-store');
 }
 for(const path of ['/api/health','/api/auth/get-session','/login','/holdings/acme/home']){
  expect(request(path).headers.get('location')).toBeNull();
 }
 expect(request('/','POST').headers.get('location')).toBeNull();
 expect(request('/','GET','127.0.0.1:3100').headers.get('location')).toBeNull();
 for(const environment of ['production','staging']){
  vi.stubEnv('ERPX_ENVIRONMENT',environment);
  expect(request('/').headers.get('location')).toBeNull();
 }
 vi.stubEnv('ERPX_ENVIRONMENT','local');
 for(const flag of ['ERPX_PRODUCT_ROUTING','ERPX_PRODUCT_SSO']){
  vi.stubEnv(flag,'disabled');
  expect(request('/').headers.get('location')).toBeNull();
  vi.stubEnv(flag,'enabled');
 }
});
it('serves shared product icons on new Front hosts without opening operational or provider routes',()=>{
 vi.stubEnv('ERPX_ENVIRONMENT','local');
 vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');
 vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 const request=(host:string,path:string)=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{headers:{host}}));
 for(const host of ['pos-dev-erpx.jesadakorn.com','pms-dev-erpx.jesadakorn.com']){
  for(const path of ['/icon.svg','/apple-icon.png','/icon-192.png']){
   const response=request(host,path);
   expect(response.status).toBe(200);
   expect(response.headers.get('x-middleware-next')).toBe('1');
   expect(response.headers.get('x-middleware-rewrite')).toBeNull();
  }
  expect(request(host,'/favicon.ico').headers.get('location')).toContain('/icon/192');
  expect(request(host,'/holdings/acme/finance').status).toBe(404);
  expect(request(host,'/api/v1/platform/operations/provision').status).toBe(404);
  expect(request(host,'/icon.svg/private').status).toBe(404);
 }
 expect(request('pos.dev-erpx.jesadakorn.com','/icon.svg').status).toBe(421);
});
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

it('allows the exact personal preference endpoint on every development product, including Front, while retaining host boundaries',()=>{
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','https://pos-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','https://pms-dev-erpx.jesadakorn.com');
 const keys=[...productIds,...sharedHostKeys];
 const request=(key:string,path:string,method='GET')=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{method,headers:{host:`${key}-dev-erpx.jesadakorn.com`}}));
 for(const key of keys)for(const method of ['GET','PATCH']){
  const response=request(key,'/api/v1/account/preferences',method);
  expect(response.status,`${key} ${method}`).toBe(200);
  expect(response.headers.get('location')).toBeNull();
  expect(response.headers.get('cache-control')).toBe('private, no-store');
 }
 for(const key of ['pos','pms','apps','booking','portal','legal-portal','forms']){
  expect(request(key,'/api/v1/account/profile').status).toBe(404);
  expect(request(key,'/api/v1/account/preferences/extra').status).toBe(404);
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

it('routes owned Front launch to its GET handler while legacy Front and owner mutation paths keep their original parser',()=>{
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');vi.stubEnv('ERPX_FRONT_POS_ORIGIN','https://dev-pos.jesadakorn.com');
 const request=(host:string,path:string)=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{headers:{host}}));
 const owned=request('pos-dev-erpx.jesadakorn.com','/launch?companyId=c1&next=review');expect(owned.headers.get('x-middleware-next')).toBe('1');expect(owned.headers.get('location')).toBeNull();expect(request('dev-pos.jesadakorn.com','/launch').status).toBe(404);expect(request('pos-dev-erpx.jesadakorn.com','/api/v1/platform/commercial').status).toBe(404);expect(request('pos-dev-erpx.jesadakorn.com','/api/v1/holdings/acme/finance/invoices').status).toBe(404);
});


it('serves same-owner backoffice and scoped commands only on the new Front product host, replacing forged Front scope without mutation redirects',()=>{
 vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
 vi.stubEnv('ERPX_FRONT_POS_ORIGIN','https://dev-pos.jesadakorn.com');vi.stubEnv('ERPX_FRONT_PMS_ORIGIN','https://dev-pms.jesadakorn.com');
 const request=(app:string,path:string,method='GET',legacy=false)=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{method,headers:{host:legacy?'dev-'+app+'.jesadakorn.com':app+'-dev-erpx.jesadakorn.com','x-erpx-front-company':'forged','x-erpx-front-app':app==='pos'?'pms':'pos','x-erpx-workspace-kind':'holding'}}));
 for(const app of ['pos','pms']){
  const path='/holdings/acme/branches/b1/'+app+'/settings?property=h1&tag=one&tag=two&_rsc=private';
  const page=request(app,path);expect(page.status).toBe(200);expect(page.headers.get('x-middleware-rewrite')).toContain('/holdings/acme/'+app+'/settings?');expect(page.headers.get('x-middleware-request-x-erpx-workspace-kind')).toBe('branch');expect(page.headers.get('x-middleware-request-x-erpx-front-company')).toBeNull();expect(page.headers.get('x-middleware-request-x-erpx-front-app')).toBeNull();expect(page.headers.get('x-middleware-request-x-erpx-page-path')).toBe(path.replace('&_rsc=private',''));expect(page.headers.get('cache-control')).toBe('private, no-store');
  for(const method of ['GET','POST','PATCH','DELETE']){
   const api=request(app,'/api/v1/holdings/acme/branches/b1/'+app,method);expect(api.status).toBe(200);expect(api.headers.get('location')).toBeNull();expect(api.headers.get('x-middleware-request-x-erpx-method')).toBe(method);
  }
  for(const shared of ['navigation','assignments/active','money','attachment-sets','attachment-links/link1/content','records/search'])expect(request(app,'/api/v1/holdings/acme/'+shared).status).toBe(200);
  expect(request(app,'/choose-work-role?companyId=c1&next=review').headers.get('x-middleware-next')).toBe('1');
  expect(request(app,path,'GET',true).status).toBe(404);expect(request(app,'/api/v1/holdings/acme/'+app,'POST',true).status).toBe(404);expect(request(app,'/choose-work-role','GET',true).status).toBe(404);
  for(const other of [app==='pos'?'pms':'pos','finance','organization','iam','manage-erpx']){
   expect(request(app,'/holdings/acme/'+other+'/settings').status).toBe(404);expect(request(app,'/api/v1/holdings/acme/'+other,'POST').status).toBe(404);
  }
 }
 expect(request('pms','/api/v1/holdings/acme/hospitality/setup','POST').status).toBe(200);expect(request('pos','/api/v1/holdings/acme/hospitality/setup','POST').status).toBe(404);
});

function enableDevelopmentProducts() {
 vi.stubEnv('ERPX_PUBLIC_ORIGIN','https://console-dev-erpx.jesadakorn.com');
 vi.stubEnv('ERPX_ENVIRONMENT','local');
 vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');
 vi.stubEnv('ERPX_PRODUCT_SSO','enabled');
}

it('rejects cross-owner Office APIs before dispatch without redirects, cookies or forged workspace scope, and preserves native dependencies',()=>{
 enableDevelopmentProducts();vi.stubEnv('__NEXT_NO_MIDDLEWARE_URL_NORMALIZE','true');
 const request=(host:string,path:string,method='POST')=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{method,headers:{host:`${host}-dev-erpx.jesadakorn.com`,'x-erpx-workspace-kind':'holding','x-erpx-branch-slug':'forged','x-erpx-department-slug':'forged'}}));
 for(const host of ['work','finance','people','sales','supply','assets','projects','recruit','learn','support','service','legal','manufacturing','admin']){
  const other=host==='finance'?'people/payroll/runs':'finance/journals';
  for(const scope of ['', 'branches/main/','departments/team/','branches/main/departments/team/'])for(const method of ['GET','HEAD','POST','PATCH','DELETE','OPTIONS']){
   const response=request(host,`/api/v1/holdings/acme/${scope}${other}?tag=one&tag=two`,method);
   expect(response.status,`${host} ${method}`).toBe(404);
   for(const header of ['location','set-cookie','x-middleware-next','x-middleware-rewrite','x-middleware-request-x-erpx-workspace-kind'])expect(response.headers.get(header)).toBeNull();
   expect(response.headers.get('cache-control')).toBe('private, no-store');
  }
 }
 for(const [host,resource] of [['projects','finance/project-cost-allocations'],['work','leave-requests/leave1/approve'],['recruit','people/hire-handoffs/consume'],['admin','pos/connection/oauth']]){
  const response=request(host,`/api/v1/holdings/acme/branches/main/departments/team/${resource}?tag=one&tag=two`);
  expect(response.status,host).toBe(200);
  expect(response.headers.get('location')).toBeNull();
  expect(response.headers.get('x-middleware-rewrite')).toBe(`http://127.0.0.1:3100/api/v1/holdings/acme/${resource}?tag=one&tag=two`);
  expect(response.headers.get('x-middleware-request-x-erpx-workspace-kind')).toBe('department');
  expect(response.headers.get('x-middleware-request-x-erpx-branch-slug')).toBe('main');
  expect(response.headers.get('x-middleware-request-x-erpx-department-slug')).toBe('team');
 }
});
it('retires every Console page, login, callback, asset and API method before rewrites or owner dispatch',async()=>{
 enableDevelopmentProducts();vi.stubEnv('ERPX_CONSOLE_RETIRED','enabled');
 const paths=['/','/login','/activate?token=private-token','/holdings/acme/support','/holdings/acme/branches/bangkok/departments/finance/finance/invoices?new=1','/api/auth/sign-in/email','/api/v1/holdings/acme/pos','/api/pos/stripe/webhook','/api/v1/stripe/webhook','/sso/callback?code=private-code','/_next/static/chunks/app.js','/_next/image?url=private-image','/vendor/file.js','/erpx-push-sw.js','/manifest.webmanifest','/api/health/','/api/ready/extra'];
 for(const path of paths)for(const method of ['GET','HEAD','POST','PUT','PATCH','DELETE','OPTIONS']){
  const response=proxy(new NextRequest('http://127.0.0.1:3100'+path,{method,headers:{host:'console-dev-erpx.jesadakorn.com','next-action':'old-action','x-erpx-workspace-kind':'holding'}}));
  expect(response.status).toBe(404);expect(response.headers.get('location')).toBeNull();
  expect(response.headers.get('x-middleware-next')).toBeNull();expect(response.headers.get('x-middleware-rewrite')).toBeNull();expect(response.headers.get('set-cookie')).toBeNull();
  expect(response.headers.get('cache-control')).toBe('private, no-store');expect(response.headers.get('referrer-policy')).toBe('no-referrer');
  const body=await response.text();expect(body).not.toMatch(/private-token|private-code|private-image/);
  if(method==='HEAD')expect(body).toBe('');else expect(body).toBe('Not Found\n');
 }
 const json=proxy(new NextRequest('http://127.0.0.1:3100/api/v1/holdings/acme/finance',{method:'POST',headers:{host:'console-dev-erpx.jesadakorn.com',accept:'application/json'}}));
 expect(json.headers.get('content-type')).toBe('text/plain; charset=utf-8');
 expect(await json.text()).toBe('Not Found\n');
});
it('preserves health and ready, rejects foreign hosts and cannot reopen retired Console through an SSO outage',async()=>{
 enableDevelopmentProducts();vi.stubEnv('ERPX_CONSOLE_RETIRED','enabled');
 const request=(path:string,host='console-dev-erpx.jesadakorn.com')=>proxy(new NextRequest('http://127.0.0.1:3100'+path,{headers:{host}}));
 for(const path of ['/api/health','/api/ready'])expect(request(path).headers.get('x-middleware-next')).toBe('1');
 for(const path of ['/api/health','/api/ready'])for(const method of ['POST','PUT','PATCH','DELETE','OPTIONS'])expect(proxy(new NextRequest('http://127.0.0.1:3100'+path,{method,headers:{host:'console-dev-erpx.jesadakorn.com'}})).status).toBe(404);
 for(const path of ['/api/health','/api/ready'])for(const method of ['GET','HEAD']){
  const health=new NextRequest('http://127.0.0.1:3217'+path+'?probe=read-only',{method,headers:{host:'console-dev-erpx.jesadakorn.com'}});
  const response=await accountProxy(health);expect(response.status).toBe(200);expect(response.headers.get('x-middleware-next')).toBe('1');expect(response.headers.get('x-erpx-app')).toBe('account');
  expect((await appProxy('finance')(health)).status).toBe(421);
 }
 for(const path of ['/api/health/','/api/ready/extra','/login','/api/auth/sign-in/email'])expect((await accountProxy(new NextRequest('http://127.0.0.1:3217'+path,{headers:{host:'console-dev-erpx.jesadakorn.com'}}))).status).toBe(421);
 for(const method of ['POST','PUT','PATCH','DELETE','OPTIONS'])expect((await accountProxy(new NextRequest('http://127.0.0.1:3217/api/health',{method,headers:{host:'console-dev-erpx.jesadakorn.com'}}))).status).toBe(421);

 expect(request('/','unknown.example').status).toBe(421);expect(request('/api/health','127.0.0.1:3100').status).toBe(200);
 expect(request('/','apps-dev-erpx.jesadakorn.com').status).toBe(200);
 for(const flag of ['ERPX_PRODUCT_ROUTING','ERPX_PRODUCT_SSO']){vi.stubEnv(flag,'disabled');expect(request('/login').status).toBe(404);vi.stubEnv(flag,'enabled');}
 for(const environment of ['staging','production']){vi.stubEnv('ERPX_ENVIRONMENT',environment);expect(request('/login').status).toBe(200);expect((await accountProxy(new NextRequest('http://127.0.0.1:3217/api/health',{headers:{host:'console-dev-erpx.jesadakorn.com'}}))).status).toBe(421);}
 vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_CONSOLE_RETIRED','disabled');expect(request('/login').status).toBe(200);
 for(const path of ['/_next/static/chunks/app.js','/_next/image?url=image','/vendor/file.js']){
  const response=request(path,'pos-dev-erpx.jesadakorn.com');expect(response.headers.get('cache-control')).toBeNull();expect(response.headers.get('x-middleware-next')).toBe('1');
 }
});
it('resolves marketing and account invitation entry on registry owners with scope and repeated filters intact',()=>{
 enableDevelopmentProducts();vi.stubEnv('ERPX_MARKETING_ORIGIN','https://dev-erpx.jesadakorn.com');
 const paths={apps:'/',account:'/activate',admin:'/holdings/acme/administration/organization',booking:'/book/acme',portal:'/customer-portal/acme','legal-portal':'/legal-portal/acme',forms:'/forms/acme/contact',work:'/holdings/acme/work',pos:'/holdings/acme/pos',pms:'/holdings/acme/pms',finance:'/holdings/acme/finance/accounting/journals',people:'/holdings/acme/people-organization/people',sales:'/holdings/acme/customers-sales/sales',supply:'/holdings/acme/purchasing',assets:'/holdings/acme/services/assets',projects:'/holdings/acme/projects-routines',recruit:'/holdings/acme/people-organization/talent',learn:'/holdings/acme/knowledge/learning',support:'/holdings/acme/customers-sales/customers/service',service:'/holdings/acme/services/field-service',legal:'/holdings/acme/services/legal',manufacturing:'/holdings/acme/production'};
 for(const [owner,path] of Object.entries(paths)){
  const url=applicationUrl(path,'?tag=one&tag=two');expect(url.origin).toBe(`https://${owner}-dev-erpx.jesadakorn.com`);expect(url.searchParams.getAll('tag')).toEqual(['one','two']);
 }
 const url=applicationUrl('/holdings/acme/branches/bangkok/departments/payables/finance/accounting/journals','?q=open&record=original');
 expect(url.href).toBe('https://finance-dev-erpx.jesadakorn.com/holdings/acme/branches/bangkok/departments/payables/finance/accounting/journals?q=open&record=original');
 expect(applicationUrl('//foreign.example/path','?token=original').origin).toBe('https://apps-dev-erpx.jesadakorn.com');
 const response=proxy(new NextRequest('http://127.0.0.1:3100/book/acme?arrival=2026-10-01',{headers:{host:'dev-erpx.jesadakorn.com'}}));expect(response.headers.get('location')).toBe('https://booking-dev-erpx.jesadakorn.com/book/acme?arrival=2026-10-01');
 for(const path of ['/activate','/join','/api/v1/account-invitations','/api/v1/join'])expect(proxy(new NextRequest('http://127.0.0.1:3100'+path,{headers:{host:'account-dev-erpx.jesadakorn.com'}})).status).toBe(200);
 vi.stubEnv('ERPX_PRODUCT_ROUTING','disabled');expect(applicationUrl('/book/acme').href).toBe('https://console-dev-erpx.jesadakorn.com/book/acme');
});
it('replaces only known pending Console recovery destinations while retaining original single-use tokens',()=>{
 enableDevelopmentProducts();const legacy='https://console-dev-erpx.jesadakorn.com',account='https://account-dev-erpx.jesadakorn.com';
 const original=legacy+'/api/auth/reset-password/original-token?callbackURL='+encodeURIComponent(legacy+'/reset-password');
 const migrated=new URL(accountRecoveryDeliveryUrl(original));expect(migrated.origin).toBe(account);expect(migrated.pathname).toBe('/api/auth/reset-password/original-token');expect(migrated.searchParams.get('callbackURL')).toBe(account+'/reset-password');
 for(const value of ['https://foreign.example/reset-password?token=original',legacy+'/api/auth/callback/directory-original?code=original',account+'/reset-password?token=original'])expect(accountRecoveryDeliveryUrl(value)).toBe(value);
 expect(applicationOrigin('account','https://isolated.example.test')).toBe('https://isolated.example.test');
 vi.stubEnv('ERPX_PRODUCT_ROUTING','disabled');expect(accountRecoveryDeliveryUrl(original)).toBe(original);
});
it('returns POS provider setup through the actual Admin Company launch without carrying callback credentials',()=>{
 enableDevelopmentProducts();
 const href=new URL(adminCompanyHandoff('original-company','/holdings/acme/settings/stripe','https://pos-dev-erpx.jesadakorn.com'));
 expect(href.origin).toBe('https://admin-dev-erpx.jesadakorn.com');expect(href.pathname).toBe('/sso/start');
 const launch=new URL(href.searchParams.get('next')!,'https://admin-dev-erpx.jesadakorn.com');
 expect(launch.pathname).toBe('/admin/launch');expect(launch.searchParams.get('companyId')).toBe('original-company');expect(launch.searchParams.get('next')).toBe('/holdings/acme/administration/stripe');
 expect(adminCompanyHandoff('original-company','/holdings/acme/settings/stripe','https://isolated.example.test')).toBe('https://isolated.example.test/holdings/acme/administration/stripe');
});
