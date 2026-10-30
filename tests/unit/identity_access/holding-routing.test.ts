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

it('retires former product hosts only for the explicit Holding profile', async () => {
  const { appForHost, unregisteredDevelopmentProductHost } = await import('@erpx/contracts/apps');
  const enabled = {ERPX_HOLDING_ROUTING:'enabled',ERPX_ENVIRONMENT:'local',ERPX_PRODUCT_ENVIRONMENT:'development'};
  expect(appForHost('finance-dev-erpx.jesadakorn.com')?.id).toBe('finance');
  expect(appForHost('finance-dev-erpx.jesadakorn.com',enabled)).toBeUndefined();
  expect(unregisteredDevelopmentProductHost('finance-dev-erpx.jesadakorn.com',enabled)).toBe(true);
  expect(unregisteredDevelopmentProductHost('finance-dev-erpx.jesadakorn.com')).toBe(false);
  expect(appForHost('account-dev-erpx.jesadakorn.com',enabled)?.id).toBe('account');
});

it('restricts the contributor profile to its four exact localhost origins', async () => {
  const {activeHoldingHosts, accountDevelopmentOrigin, contributorExactOrigin} = await import('@erpx/contracts/holding-hosts');
  const hosts = ['account.erpx.localhost:3443', 'acme.erpx.localhost:3443', 'northstar.erpx.localhost:3443', 'erpx-internal.erpx.localhost:3443'];
  expect(accountDevelopmentOrigin('contributor')).toBe(`https://${hosts[0]}`);
  expect(activeHoldingHosts('contributor').map(item => item.hostname)).toEqual(hosts.slice(1));
  for (const host of hosts) {
    expect(contributorExactOrigin(host, 'contributor')).toBe(`https://${host}`);
    expect(contributorExactOrigin(host)).toBeNull();
  }
  for (const host of ['acme.erpx.localhost', 'acme.erpx.localhost:443', 'acme.erpx.localhost:3443.evil.test', 'unknown.erpx.localhost:3443'])
    expect(contributorExactOrigin(host, 'contributor')).toBeNull();
  expect(holdingHostForHost(hosts[1])).toBeUndefined();
  expect(holdingHostForHost(hosts[1], 'contributor')?.holdingId).toBe('h001');
  expect(holdingUiUrl('/holdings/acme/finance/documents', 'acme-co', 'contributor')).toBe('https://acme.erpx.localhost:3443/finance?company=acme-co');
});

it('preserves request data when routing exact contributor browser API calls', async () => {
  const {holdingFetch} = await import('@erpx/contracts/holding-fetch');
  const fetchMock = vi.fn<(...args: Parameters<typeof fetch>) => ReturnType<typeof fetch>>(async () => new Response('ok'));
  vi.stubGlobal('fetch', fetchMock);
  vi.stubGlobal('window', {location: {host: 'acme.erpx.localhost:3443', origin: 'https://acme.erpx.localhost:3443', href: 'https://acme.erpx.localhost:3443/finance'}});
  vi.stubGlobal('document', {documentElement: {dataset: {erpxApp: 'finance'}}});
  const request = new Request('https://acme.erpx.localhost:3443/api/v1/payments?x=1', {method: 'POST', headers: {'content-type': 'text/plain', 'idempotency-key': 'key-123'}, body: 'raw-body'});
  await holdingFetch(request);
  const routed = fetchMock.mock.calls[0][0] as Request;
  expect(routed.url).toBe('https://acme.erpx.localhost:3443/__erpx/finance/api/v1/payments?x=1');
  expect(routed.method).toBe('POST');
  expect(routed.headers.get('idempotency-key')).toBe('key-123');
  expect(routed.headers.get('content-type')).toBe('text/plain');
  expect(await routed.text()).toBe('raw-body');
  await holdingFetch('/api/v1/payments', {method: 'POST', headers: {'idempotency-key': 'key-456'}, body: 'raw-init'});
  expect(String(fetchMock.mock.calls[1][0])).toBe('https://acme.erpx.localhost:3443/__erpx/finance/api/v1/payments');
  expect(fetchMock.mock.calls[1][1]).toMatchObject({body: 'raw-init', headers: {'idempotency-key': 'key-456'}});
  vi.stubGlobal('window', {location: {host: 'unknown.erpx.localhost:3443', origin: 'https://unknown.erpx.localhost:3443', href: 'https://unknown.erpx.localhost:3443/finance'}});
  await holdingFetch('/api/v1/payments');
  expect(fetchMock.mock.calls[2][0]).toBe('/api/v1/payments');
});

it('maps Holding navigation using the explicitly supplied local origin', () => {
  const scope = {audience: 'finance' as const, holdingSlug: 'acme', holdingOrigin: 'https://acme.erpx.localhost:3443', companySlug: 'acme-co'};
  expect(productNavigationHref('/holdings/acme/finance/reports/company', scope)).toEqual({href: 'https://acme.erpx.localhost:3443/finance/reports/company?company=acme-co', handoff: false});
  expect(productNavigationHref('/bangkok/people', scope)).toEqual({href: 'https://acme.erpx.localhost:3443/bangkok/people', handoff: true});
  expect(productNavigationHref('/holdings/acme/finance/reports/company', {...scope, holdingOrigin: 'https://acme.erpx.localhost:3443.evil.test'}).href).toBe('https://acme.jesadakorn.com/finance/reports/company?company=acme-co');
});
