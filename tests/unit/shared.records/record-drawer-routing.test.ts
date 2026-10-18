import {expect,it} from 'vitest';
import {existsSync,readFileSync} from 'node:fs';
import {recordTarget,recordDrawerHref,recordCanonicalHref,withoutRecordDrawer} from '../../../src/lib/records/record-drawer';
import {canonicalUiHref,destinations,destinationTrail} from '../../../src/lib/work_projection/destination-registry';
import {canonicalDestinationLinks} from '../../../src/lib/work_projection/destination-links';
import {destinationProduct} from '../../../src/lib/commercial/product-registry';
import {productPageAllowed} from '../../../src/lib/runtime/product-route-policy';
import {productEntryCapability} from '../../../src/server/modules/commercial/public/rules/product-entry-capability';
const base='/holdings/acme';
it('keeps repeated collection filters, pagination and focus fragment when closing a path drawer',()=>{
 const current=`${base}/projects-routines/projects?q=desk&filter=a&filter=b&cursor=page2#projects`;
 const selected=recordDrawerHref(current,{kind:'project',id:'project:line / encoded',args:{section:'documents'}});
 expect(new URL(selected,'https://test.invalid').pathname).toBe(`${base}/projects-routines/projects/project%3Aline%20%2F%20encoded/documents`);
 expect(withoutRecordDrawer(selected)).toBe(current);
 expect(new URL(selected,'https://test.invalid').searchParams.has('detailId')).toBe(false);
});
it('opens the same record and section from a direct or modified-click link',()=>{
 const target={kind:'stock_position' as const,id:'product:1',args:{warehouseId:'warehouse:2'}};
 expect(recordTarget(recordCanonicalHref(base,target),base)).toEqual({...target,args:{...target.args,section:'overview'}});
 expect(recordTarget(`${base}/services/legal/contract/a%3Ab/documents`,base)).toEqual({kind:'legal',id:'a:b',args:{kind:'contract',section:'documents'}});
 expect(recordTarget(`${base}/people-organization/people/directory/person-id/pay`,base)?.args?.section).toBe('pay');
});
it('keeps print, external origins and other work scopes outside the drawer',()=>{
 expect(recordTarget(`${base}/finance/documents/invoices/1/overview?print=1`,base)).toBeNull();
 expect(recordTarget('https://example.com/holdings/acme/finance/documents/invoices/1/overview',base)).toBeNull();
 expect(recordTarget('/holdings/another/finance/documents/invoices/1/overview',base)).toBeNull();
});
it('does not mistake submenu and creation routes for record IDs',()=>{
 for(const path of ['people/organization','people/payroll','support/incidents','support/sla','people/move','maintenance/plans','requests/new','people-organization/people/directory/current','administration/technology/directory/connections'])expect(recordTarget(`${base}/${path}`,base)).toBeNull();
});
it('normalizes generated owner links and keeps APIs, filters and workspace scope',()=>{
 expect(canonicalUiHref(`${base}/branches/bangkok/departments/sales/technology?view=directory&section=connections&q=Thai&tag=a&tag=b`)).toBe(`${base}/branches/bangkok/departments/sales/administration/technology/directory/connections?q=Thai&tag=a&tag=b`);
 expect(canonicalUiHref(`${base}/finance/setup?step=books&chart=chart-1`)).toBe(`${base}/finance/setup/books?chart=chart-1`);
 expect(canonicalUiHref(`/api/v1${base}/finance/invoices?kind=payable`)).toBe(`/api/v1${base}/finance/invoices?kind=payable`);
 expect(canonicalDestinationLinks({data:{href:'/finance/payments/payment-1',version:7},api:`/api/v1${base}/finance/payments`,input:{path:'finance/payments'}})).toEqual({data:{href:'/finance/cash/payments/payment-1/overview',version:7},api:`/api/v1${base}/finance/payments`,input:{path:'finance/payments'}});
});
it('provides distinct Server Component pages and clickable breadcrumb parents for every destination',()=>{
 const paths=destinations.map(destination=>destination.path);
 expect(new Set(paths).size).toBe(paths.length);
 for(const path of paths){const file=`src/app/holdings/[holdingSlug]/${path}/page.tsx`;expect(existsSync(file),path).toBe(true);expect(readFileSync(file,'utf8')).not.toMatch(/^['"]use client['"]/);for(const crumb of destinationTrail(base,path).slice(0,-1))expect(existsSync(`src/app/holdings/[holdingSlug]/${crumb.href!.slice(base.length+1)}/page.tsx`),crumb.href).toBe(true);}
 expect(existsSync('src/app/holdings/[holdingSlug]/technology/page.tsx')).toBe(false);
 expect(existsSync('src/app/platform/billing/page.tsx')).toBe(false);
});

it('preserves canonical Purchase and Quotation record identity across repeated normalization and drawer navigation',()=>{
 for(const scope of [base,base+'/branches/bangkok/departments/purchasing'])for(const [view,path] of [['purchase','purchasing/orders'],['quote','customers-sales/sales/documents/quote'],['sales','customers-sales/sales/documents/sales']] as const){
  const legacy=`${scope}/commerce/orders/original-order?view=${view}&tag=one&tag=two&cursor=page2#evidence`,expected=`${scope}/${path}/original-order/overview?tag=one&tag=two&cursor=page2#evidence`;
  expect(canonicalUiHref(legacy)).toBe(expected);
  expect(canonicalUiHref(expected)).toBe(expected);
  expect(canonicalUiHref(canonicalUiHref(expected))).toBe(expected);
  const drawer=recordDrawerHref(`${scope}/${path}?tag=one&tag=two&cursor=page2#records`,{kind:'order',id:'original-order',args:{view}});
  expect(canonicalUiHref(drawer)).toBe(drawer);
  expect(new URL(canonicalUiHref(drawer),'https://test.invalid').pathname).toBe(`${scope}/${path}/original-order/overview`);
  expect(withoutRecordDrawer(drawer)).toBe(`${scope}/${path}?tag=one&tag=two&cursor=page2#records`);
 }
});

it('keeps exact accepted hire handoffs on their owner page instead of treating the path as a Person ID',()=>{
 for(const scope of [base,base+'/branches/bangkok'])for(const section of ['', '/overview']){
  const href=`${scope}/people-organization/people/hire-handoffs/original-handoff${section}?tag=one&tag=two#receipt`;
  expect(canonicalUiHref(href)).toBe(href);expect(canonicalUiHref(canonicalUiHref(href))).toBe(href);
 }
});

it('keeps the accepted hire collection separate from an ordinary Person record across normalization',()=>{
 for(const scope of [base,base+'/branches/bangkok/departments/people']){
  const query='?tag=one&tag=two&cursor=page2#handoffs',expected=`${scope}/people-organization/people/hire-handoffs${query}`;
  expect(canonicalUiHref(`${scope}/people/hire-handoffs${query}`)).toBe(expected);
  expect(canonicalUiHref(expected)).toBe(expected);
  expect(canonicalUiHref(canonicalUiHref(expected))).toBe(expected);
  expect(recordTarget(expected,scope)).toBeNull();
 }
});

it('keeps the exact candidate privacy page and its query separate from ordinary Talent records',()=>{
 for(const suffix of ['', '/overview']){const href=`${base}/people-organization/talent/candidates/original-candidate/privacy${suffix}?tag=one&tag=two#history`;expect(canonicalUiHref(href)).toBe(href);expect(canonicalUiHref(canonicalUiHref(href))).toBe(href);}
});
it('keeps original price-list identity and drawer return filters on Sales instead of requiring the Finance product',()=>{
 for(const scope of [base,base+'/branches/main']){const current=`${scope}/customers-sales/sales/pricing?tag=one&tag=two#prices`,target={kind:'price_list' as const,id:'list:original /version'},href=recordDrawerHref(current,target),url=new URL(href,'https://test.invalid');expect(url.pathname).toBe(`${scope}/customers-sales/sales/pricing/records/price_list/list%3Aoriginal%20%2Fversion/overview`);expect(destinationProduct(url.pathname.slice(scope.length+1))).toBe('sales');expect(canonicalUiHref(href)).toBe(href);expect(canonicalUiHref(canonicalUiHref(href))).toBe(href);expect(recordTarget(href,scope)).toEqual({...target,args:{returnTo:current,section:'overview'}});expect(withoutRecordDrawer(href)).toBe(current);expect(canonicalUiHref(`${scope}/record-details/price_list/original?tag=one&tag=two`)).toBe(`${scope}/customers-sales/sales/pricing/records/price_list/original/overview?tag=one&tag=two`);}
 for(const suffix of ['[id]','[id]/[recordSection]'])for(const slot of ['', '@record/'])expect(existsSync(`src/app/holdings/[holdingSlug]/${slot}customers-sales/sales/pricing/records/price_list/${suffix}/page.tsx`)).toBe(true);
 const path='customers-sales/sales/pricing/records/price_list/original/overview';expect(productPageAllowed('sales',base+'/'+path)).toBe(true);expect(productPageAllowed('finance',base+'/'+path)).toBe(false);expect(productEntryCapability('sales',path)).toBe('sales.orders');
});
