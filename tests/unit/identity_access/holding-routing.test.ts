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
