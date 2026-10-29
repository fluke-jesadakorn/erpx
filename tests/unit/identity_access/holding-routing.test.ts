import {describe,it,expect,vi,afterEach} from 'vitest';
import {workspacePathParts} from '@erpx/feature-work_projection/lib/destination-registry';
import {recordTarget,withoutRecordDrawer} from '@erpx/feature-records/lib/record-drawer';
import {productNavigationHref} from '@erpx/feature-work_projection/lib/product-navigation';
afterEach(()=>vi.unstubAllGlobals());
import {holdingHostForHost,holdingAudience,holdingRoutingEnabled,holdingUiUrl,holdingPublicUiUrl,holdingFrontUiUrl,parseHoldingRoute,holdingPathMatches} from '@erpx/contracts/holding-hosts';
import {holdingGatewayApp,holdingGatewayLocation} from '../../../scripts/local/holding-gateway';

describe('Holding addresses',()=>{
 it('trusts only exact development registrations and keeps staging and future origins out',()=>{
  expect(holdingHostForHost('acme.jesadakorn.com')?.holdingId).toBe('h001');
  for(const host of ['acme.erpx.com','erpx.jesadakorn.com','unknown.jesadakorn.com','acme.jesadakorn.com.evil.test','ACME.jesadakorn.com','acme.jesadakorn.com:443'])expect(holdingHostForHost(host)).toBeUndefined();
  expect(holdingRoutingEnabled({ERPX_HOLDING_ROUTING:'enabled',ERPX_ENVIRONMENT:'production',ERPX_PRODUCT_ENVIRONMENT:'development'})).toBe(false);
 });
 it('binds the session audience to the exact Holding host',()=>{
  expect(holdingAudience('acme.jesadakorn.com')).toBe('holding:development:h001:acme.jesadakorn.com');
  expect(holdingAudience('northstar.jesadakorn.com')).not.toBe(holdingAudience('acme.jesadakorn.com'));
  expect(holdingAudience('finance-dev-erpx.jesadakorn.com')).toBeNull();
 });
 it.each([
  ['/bangkok/finance','finance','/holdings/acme/branches/bangkok/finance/documents'],
  ['/bangkok/accounting/finance','finance','/holdings/acme/branches/bangkok/departments/accounting/finance/documents'],
  ['/departments/accounting/people','people','/holdings/acme/departments/accounting/people-organization/people/directory/current'],
  ['/bangkok/finance/reports/company','finance','/holdings/acme/branches/bangkok/finance/reports/company'],
  ['/forms/survey','forms','/forms/acme/survey'],
  ['/book/manage','booking','/book/acme/manage'],
  ['/__erpx/pos/front/acme/acme-co/logo','pos','/front/acme/acme-co/pos/logo'],
 ])('maps %s to its owner without losing scope',(path,app,internal)=>{
  const route=parseHoldingRoute('acme',path);expect(route?.app).toBe(app);expect(route?.internalPath).toBe(internal);
 });
 it('opens a picker when an app has no explicit workspace',()=>{
  expect(parseHoldingRoute('acme','/finance')).toMatchObject({kind:'entry',app:'finance'});
  expect(parseHoldingRoute('acme','/')).toMatchObject({kind:'entry',app:'apps'});
 });
 it.each(['/bangkok/finance/finance','/bangkok/%2f/finance','/bangkok/%5c/finance','/provider','/__erpx/account/api/auth/sign-in/email','/__erpx/finance/api/v1/holdings/northstar/finance'])('rejects ambiguous or foreign path %s',path=>{
  // A finance app tail is an owner path; a colliding department is never inferred.
  if(path==='/bangkok/finance/finance')expect(parseHoldingRoute('acme',path)?.departmentSlug).toBeUndefined();
  else expect(parseHoldingRoute('acme',path)).toBeNull();
 });
 it('preserves record suffixes, repeated filters, Company and fragments in generated links',()=>{
  expect(holdingUiUrl('/holdings/acme/branches/bangkok/departments/accounting/finance/reports/company?tag=a&tag=b#totals')).toBe('https://acme.jesadakorn.com/bangkok/accounting/finance/reports/company?tag=a&tag=b#totals');
  expect(holdingUiUrl('/holdings/acme/finance/documents','acme-co')).toBe('https://acme.jesadakorn.com/finance?company=acme-co');
  expect(holdingPublicUiUrl('/book/acme/manage#token=original')).toBe('https://acme.jesadakorn.com/book/manage#token=original');
  expect(holdingFrontUiUrl('/acme/acme-co/branches/bangkok/sale?table=1','pos')).toBe('https://acme.jesadakorn.com/bangkok/pos/sale?table=1');
 });
 it('routes APIs and launch requests to a declared app without granting authority',()=>{
  expect(holdingGatewayApp('acme.jesadakorn.com','/launch?app=finance&next=%2Fholdings%2Facme%2Ffinance')).toBe('finance');
  expect(holdingGatewayApp('acme.jesadakorn.com','/__erpx/people/api/v1/holdings/acme/people')).toBe('people');
  expect(holdingGatewayApp('acme.jesadakorn.com','/api/v1/holdings/acme/finance')).toBe('finance');
  expect(holdingGatewayApp('acme.jesadakorn.com','/api/v1/holdings/northstar/finance')).toBeNull();
  expect(holdingPathMatches('acme','/api/v1/front/northstar/northstar-co/pos')).toBe(false);
 });
 it('retains explicit Company scope in owner redirects and leaves Account exchange untouched',()=>{
  expect(holdingGatewayLocation('acme.jesadakorn.com','/finance?company=acme-co','/holdings/acme/finance/documents/sales/invoices','finance')).toBe('https://acme.jesadakorn.com/finance/documents/sales/invoices?company=acme-co');
  expect(holdingGatewayLocation('acme.jesadakorn.com','/finance?company=acme-co','https://account-dev-erpx.jesadakorn.com/sso/authorize?state=original','finance')).toBe('https://account-dev-erpx.jesadakorn.com/sso/authorize?state=original');
 });
 it('keeps drawer identity and list filters on the visible Holding workspace',()=>{
  vi.stubGlobal('window',{location:{host:'acme.jesadakorn.com'}});
  const current='/bangkok/finance/documents/invoices/original/overview',base='/holdings/acme/branches/bangkok';
  expect(workspacePathParts(current)).toEqual({base,path:'finance/documents/invoices/original/overview'});
  expect(recordTarget(current,base)).toMatchObject({kind:'invoice',id:'original'});
  const url=new URL(current,'https://path.invalid'),list='/bangkok/finance/documents/sales/invoices?tag=a&tag=b#list';
  url.searchParams.set('returnTo',list);expect(withoutRecordDrawer(url.pathname+url.search)).toBe(list);
  url.searchParams.set('returnTo','/chiang-mai/finance/documents/sales/invoices');expect(withoutRecordDrawer(url.pathname+url.search)).not.toContain('/chiang-mai/');
 });
 it('uses a document navigation for another app and retains the selected Company',()=>{
  const scope={audience:'finance' as const,holdingSlug:'acme',companySlug:'acme-co'};
  expect(productNavigationHref('/holdings/acme/finance/reports/company',scope)).toEqual({href:'https://acme.jesadakorn.com/finance/reports/company?company=acme-co',handoff:false});
  expect(productNavigationHref('/bangkok/people',scope)).toEqual({href:'https://acme.jesadakorn.com/bangkok/people',handoff:true});
 });

});

describe('isolated contributor HTTPS origins',()=>{
 afterEach(()=>vi.unstubAllEnvs());
 it('uses only the four full development authorities and keeps the public profile separate',async()=>{
  vi.stubEnv('NODE_ENV','development');vi.stubEnv('NEXT_PUBLIC_ERPX_DEV_ORIGIN_PROFILE','container-localhost');vi.stubEnv('ERPX_HOLDING_ROUTING','enabled');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');vi.stubEnv('ERPX_APP_ID','finance');
  const hosts=await import('@erpx/feature-runtime/lib/holding-hosts'),runtime=await import('@erpx/feature-runtime/lib/product-hosts'),apps=await import('@erpx/contracts/apps');
  for(const [host,id] of [['acme.erpx.localhost:3443','h001'],['northstar.erpx.localhost:3443','h002'],['erpx-internal.erpx.localhost:3443','h003']] as const){expect(hosts.holdingHostForHost(host)?.holdingId).toBe(id);expect(hosts.holdingAudience(host)).toBe(`holding:development:${id}:${host}`);expect(runtime.trustedProductOrigin(`https://${host}`)).toBe(`https://${host}`);}
  const {requestPublicOrigin}=await import('@erpx/server-manage_erpx/public/public-origin');
  const source=new Request('http://127.0.0.1:3204/path',{headers:{host:'acme.erpx.localhost:3443','x-forwarded-host':'evil.example.test','x-forwarded-proto':'http'}});expect(requestPublicOrigin(source)).toBe('https://acme.erpx.localhost:3443');
  const {ssoOrigins}=await import('@erpx/server-identity_access/adapters/product-sso-context');const origins=ssoOrigins();expect(Object.keys(origins)).toHaveLength(4);expect(origins.account).toBe('https://account.erpx.localhost:3443');expect(origins['holding:development:h001:acme.erpx.localhost:3443']).toBe('https://acme.erpx.localhost:3443');
  expect(apps.appForHost('account.erpx.localhost:3443',process.env,'contributor')?.id).toBe('account');expect(runtime.configuredProductOrigin('account')).toBe('https://account.erpx.localhost:3443');expect(hosts.holdingUiUrl('/holdings/acme/finance/documents')).toBe('https://acme.erpx.localhost:3443/finance');
  for(const host of ['acme.erpx.localhost','acme.erpx.localhost:443','acme.erpx.localhost:3444','x.acme.erpx.localhost:3443','acme.jesadakorn.com','account.erpx.localhost:3444'])expect(hosts.contributorExactOrigin(host)).toBeNull();
  expect(runtime.trustedProductOrigin('http://acme.erpx.localhost:3443')).toBeNull();expect(runtime.trustedProductOrigin('https://acme.erpx.localhost:3444')).toBeNull();
  vi.stubEnv('NODE_ENV','production');expect(hosts.holdingHostForHost('acme.jesadakorn.com')?.holdingId).toBe('h001');expect(hosts.contributorExactOrigin('acme.erpx.localhost:3443')).toBeNull();
 });
});

it('selects only the exact HTTPS contributor browser origin for presentation transport',async()=>{
 const {holdingFetch}=await import('@erpx/contracts/holding-fetch');const fetcher=vi.fn(async(_input:RequestInfo|URL,_init?:RequestInit)=>new Response(null,{status:204}));vi.stubGlobal('fetch',fetcher);vi.stubGlobal('document',{documentElement:{dataset:{erpxApp:'finance'}}});
 try {
  for(const [origin,host,expected] of [['https://acme.erpx.localhost:3443','acme.erpx.localhost:3443','/__erpx/finance/api/v1/holdings/acme/finance'],['https://acme.erpx.localhost:3444','acme.erpx.localhost:3444','/api/v1/holdings/acme/finance'],['http://acme.erpx.localhost:3443','acme.erpx.localhost:3443','/api/v1/holdings/acme/finance']] as const){
   vi.stubGlobal('window',{location:{origin,host,href:origin+'/finance'}});fetcher.mockClear();await holdingFetch('/api/v1/holdings/acme/finance');const target=fetcher.mock.calls[0][0];expect(target instanceof URL?target.pathname:typeof target==='string'?target:target.url).toBe(expected);
  }
 }finally{vi.unstubAllGlobals();}
});
import { contributorAuthResponse, shouldProjectContributorAuth } from '../../../packages/workplace/src/routes/api/auth/[...all]/contributor-auth-response';

const user = { id: 'synthetic', name: 'Synthetic', email: 'synthetic@example.test', emailVerified: true, image: null, createdAt: 'extra' };
describe('contributor Node Account projection', () => {
  it('requires the explicit profile and exact Account authority', () => {
    expect(shouldProjectContributorAuth(true, 'account.erpx.localhost:3443', 'https://account.erpx.localhost:3443')).toBe(true);
    for (const [profile, host, origin] of [[false, 'account.erpx.localhost:3443', 'https://account.erpx.localhost:3443'], [true, 'account.erpx.localhost', 'https://account.erpx.localhost:3443'], [true, 'account.erpx.localhost:3443', 'http://account.erpx.localhost:3443'], [true, 'account-dev-erpx.jesadakorn.com', 'https://account-dev-erpx.jesadakorn.com']] as const) expect(shouldProjectContributorAuth(profile, host, origin)).toBe(false);
  });
  it('returns the strict sign-in user while retaining each cookie and security header', async () => {
    const headers = new Headers({ 'content-length': '100', 'cache-control': 'private, no-store', 'x-content-type-options': 'nosniff' });
    headers.append('set-cookie', 'first=a; Secure; HttpOnly'); headers.append('set-cookie', 'second=b; Secure; HttpOnly');
    const response = await contributorAuthResponse(Response.json({ user, token: 'secret', redirect: false, url: null }, { headers }), 'sign-in');
    expect(await response.json()).toEqual({ user: { id: user.id, name: user.name, email: user.email, emailVerified: true, image: null } });
    expect(response.headers.getSetCookie()).toEqual(['first=a; Secure; HttpOnly', 'second=b; Secure; HttpOnly']);
    expect(response.headers.get('content-length')).toBeNull(); expect(response.headers.get('x-content-type-options')).toBe('nosniff');
  });
  it('projects session fields and accepts the anonymous session', async () => {
    const session = { id: 's', userId: user.id, expiresAt: '2026-10-30T12:00:00.000Z', createdAt: '2026-10-30T11:00:00.000Z', updatedAt: '2026-10-30T11:00:00.000Z', token: 'secret' };
    expect(await (await contributorAuthResponse(Response.json({ user, session }), 'session')).json()).toEqual({user:{id:user.id,name:user.name,email:user.email,emailVerified:true,image:null},session:{id:session.id,userId:session.userId,expiresAt:session.expiresAt,createdAt:session.createdAt,updatedAt:session.updatedAt}});
    expect(await (await contributorAuthResponse(Response.json(null), 'session')).json()).toBeNull();
  });
  it('passes errors and empty success through and fails malformed successes without cookies', async () => {
    const denied = new Response('denied', {status: 401}); const empty = new Response(null, {status: 204});
    expect(await contributorAuthResponse(denied, 'sign-in')).toBe(denied); expect(await contributorAuthResponse(empty, 'session')).toBe(empty);
    const malformed = await contributorAuthResponse(Response.json({user: {...user, id: ''}}, {headers: {'set-cookie':'secret=x'}}), 'sign-in');
    expect(malformed.status).toBe(502); expect(malformed.headers.getSetCookie()).toEqual([]);
  });
});
