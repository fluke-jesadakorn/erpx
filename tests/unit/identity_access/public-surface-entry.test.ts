import {afterEach,beforeEach,expect,it,vi} from 'vitest';
vi.mock('server-only',()=>({}));
const mocks=vi.hoisted(()=>({session:vi.fn(),host:'legal-portal-dev-erpx.jesadakorn.com',redirect:vi.fn((path:string)=>{throw Error('REDIRECT:'+path);}),portal:vi.fn(),legal:vi.fn()}));
vi.mock('@/server/modules/identity_access/adapters/authorization',()=>({currentSession:mocks.session}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:mocks.host}),cookies:async()=>({get:()=>undefined})}));
vi.mock('next/navigation',()=>({redirect:mocks.redirect,useRouter:()=>({refresh:vi.fn(),push:vi.fn()}),usePathname:()=>'/legal-portal/h001',useSearchParams:()=>new URLSearchParams()}));
vi.mock('@/server/modules/customer_360/public/presentation-context-queries',()=>({customerPortalPresentationQueries:mocks.portal}));
vi.mock('@/server/modules/legal/public/presentation-context-queries',()=>({legalPortalPresentationQueries:mocks.legal,legalPortalPresentationSnapshot:mocks.legal}));
vi.mock('@/server/modules/runtime/public/runtime',()=>({holdingBindingBySlug:async()=>({id:'h001'})}));
vi.mock('@/server/modules/work_projection/public/page-error',()=>({pageError:(error:unknown)=>{throw error;}}));
import {publicSurfaceTarget,publicSurfaceLaunchPath,legalEnrollmentReturnPath,legalEnrollmentEntryTarget} from '@/server/modules/identity_access/public/rules/public-surface-entry';
import {publicSurfaceOrigin,publicSurfaceLaunchRequest} from '@/server/modules/identity_access/public/public-surface-entry';
import {productNavigationHref,isProductHandoffHref} from '@/lib/work_projection/product-navigation';
import {isValidElement,type ReactElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
const origin=(host:string)=>`https://${host}-dev-erpx.jesadakorn.com`;
const request=(host='portal',next='/customer-portal/h001?order=binding#customer-order-binding')=>new Request(origin(host)+'/public/launch?'+new URLSearchParams({next}),{headers:{host:new URL(origin(host)).host}});
beforeEach(()=>{vi.clearAllMocks();vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');vi.stubEnv('ERPX_PRODUCT_ROUTING','enabled');vi.stubEnv('ERPX_PRODUCT_SSO','enabled');mocks.session.mockResolvedValue({session:{user:{id:'current-customer'}}});mocks.portal.mockRejectedValue(Error('UNAUTHENTICATED'));mocks.legal.mockRejectedValue(Error('UNAUTHENTICATED'));});
afterEach(()=>vi.unstubAllEnvs());
it('classifies only exact original tenant workflows and excludes reserved invitation and callback routes',()=>{
 for(const [path,host] of [['/book/h001','booking'],['/book/h001/property','booking'],['/book/h001/manage','booking'],['/customer-portal/h001','portal'],['/legal-portal/h001','legal-portal'],['/forms/h001/form','forms']] as const)expect(publicSurfaceTarget(path+'?tag=one&tag=two#details')).toEqual({host,target:path+'?tag=one&tag=two#details'});
 for(const path of ['/legal-portal/enroll','/legal-portal/continue','/forms/h001','/book/h001/property/other','/customer-portal/h001/other','/x/../legal-portal/h001','/forms/%68olding/form','https://evil.test/book/h001','//evil.test/forms/h001/form','/forms/h001/form?x=%00'])expect(publicSurfaceTarget(path)).toBeNull();
});
it('uses new configured guest origins only in enabled development and retains exact legacy fallback otherwise',()=>{expect(publicSurfaceOrigin('booking','https://legacy.example')).toBe(origin('booking'));expect(publicSurfaceOrigin('legal-portal','https://legacy.example')).toBe(origin('legal-portal'));expect(publicSurfaceOrigin('account',undefined)).toBe(origin('account'));vi.stubEnv('ERPX_PRODUCT_ROUTING','disabled');expect(publicSurfaceOrigin('booking','https://legacy.example')).toBe('https://legacy.example');expect(publicSurfaceOrigin('account',undefined)).toBeUndefined();});
it('routes anonymous Booking and Forms directly and authenticated tenant portals through original SSO with a nested fragment-safe return',()=>{
 for(const [path,host] of [['/book/h001/manage#token=actual.signed.token','booking'],['/forms/h001/form?tag=one&tag=two#consent','forms']] as const){const link=productNavigationHref(path,{audience:'sales'});expect(link).toEqual({href:origin(host)+path,handoff:true});expect(isProductHandoffHref(link.href)).toBe(true);}
 for(const [path,host] of [['/customer-portal/h001?order=binding#customer-order-binding','portal'],['/legal-portal/h001?share=actual-share#document','legal-portal']] as const){const link=productNavigationHref(path,{audience:'admin'}),sso=new URL(link.href),bridge=new URL(sso.searchParams.get('next')!,'https://bridge.invalid');expect(sso.origin).toBe(origin(host));expect(sso.pathname).toBe('/sso/start');expect(bridge.pathname).toBe('/public/launch');expect(bridge.searchParams.get('next')).toBe(path);expect(productNavigationHref(path,{audience:host})).toEqual({href:path,handoff:false});}
 expect(productNavigationHref('https://evil.test/forms/h001/form',{audience:'sales'}).handoff).toBe(false);expect(productNavigationHref('https://erpx.jesadakorn.com/legal-portal/h001',{audience:'legal'}).handoff).toBe(false);
});
it('preserves the signed invitation only in the Account enrollment fragment without treating reserved paths as a tenant',()=>{
 const value='/legal-portal/enroll#token=header.payload.signature';expect(legalEnrollmentEntryTarget(value)).toBe(value);expect(productNavigationHref(value,{audience:'legal'})).toEqual({href:origin('account')+value,handoff:true});expect(isProductHandoffHref(origin('account')+value)).toBe(true);expect(productNavigationHref('/legal-portal/continue?holding=h001&share=share',{audience:'legal'}).handoff).toBe(false);
 for(const path of ['/legal-portal/enroll?token=secret','/legal-portal/enroll#token=a&token=b','/legal-portal/enroll#token=a&next=evil','/legal-portal/enroll#token=','/legal-portal/enroll#token=%5c'])expect(legalEnrollmentEntryTarget(path)).toBeNull();
});
it('validates exact host/current child session before the tenant bridge and uses normal SSO recovery without creating a role or cookie',async()=>{
 const response=await publicSurfaceLaunchRequest(request());expect(response.status).toBe(303);expect(response.headers.get('location')).toBe(origin('portal')+'/customer-portal/h001?order=binding#customer-order-binding');expect(response.headers.get('set-cookie')).toBeNull();expect(response.headers.get('cache-control')).toBe('private, no-store');mocks.session.mockRejectedValue(Error('UNAUTHENTICATED'));const recovery=await publicSurfaceLaunchRequest(request()),sso=new URL(recovery.headers.get('location')!);expect(sso.origin).toBe(origin('portal'));expect(sso.pathname).toBe('/sso/start');expect(sso.searchParams.get('next')).toBe(publicSurfaceLaunchPath('/customer-portal/h001?order=binding#customer-order-binding'));
});
it('rejects cross-surface, duplicate, foreign-host and mutation bridge requests before reading a session',async()=>{
 expect((await publicSurfaceLaunchRequest(request('forms','/forms/h001/form'))).status).toBe(404);expect((await publicSurfaceLaunchRequest(request('portal','/legal-portal/h001'))).status).toBe(422);const duplicate=new Request(origin('portal')+'/public/launch?next=/customer-portal/h001&next=/customer-portal/h002',{headers:{host:new URL(origin('portal')).host}});expect((await publicSurfaceLaunchRequest(duplicate)).status).toBe(422);expect((await publicSurfaceLaunchRequest(new Request(request(),{method:'POST',body:'{}'}))).status).toBe(405);expect(mocks.session).not.toHaveBeenCalled();
});
it('keeps the exact Legal share through unauthenticated tenant login and Customer Portal recovers to its tenant instead of a Platform workspace',async()=>{
 const {default:LegalPage}=await import('@/app/legal-portal/[holdingSlug]/page'),{default:CustomerPage}=await import('@/app/customer-portal/[holdingSlug]/page');await expect(LegalPage({params:Promise.resolve({holdingSlug:'h001'}),searchParams:Promise.resolve({share:'actual-share'})})).rejects.toThrow('REDIRECT:');expect(mocks.redirect).toHaveBeenLastCalledWith('/login?next='+encodeURIComponent('/legal-portal/h001?share=actual-share'));await expect(CustomerPage({params:Promise.resolve({holdingSlug:'h001'})})).rejects.toThrow('REDIRECT:');expect(mocks.redirect).toHaveBeenLastCalledWith('/login?'+new URLSearchParams({next:'/customer-portal/h001'}));
});
it('shows a central-account continuation from the actual legacy enrollment Server page and does not serialize an invitation token or product-origin password form',async()=>{
 mocks.host='legal-portal-dev-erpx.jesadakorn.com';const {default:Page}=await import('@/app/legal-portal/enroll/page'),page=await Page();expect(isValidElement(page)).toBe(true);const props=(page as ReactElement<{centralEnrollmentHref?:string}>).props;expect(props.centralEnrollmentHref).toBe(origin('account')+'/legal-portal/enroll');const html=renderToStaticMarkup(page);expect(html).toContain('Continue on ERPX Account');expect(html).not.toContain('type="password"');expect(html).not.toContain('token=');mocks.host='account-dev-erpx.jesadakorn.com';const central=await Page();expect(central.props.centralEnrollmentHref).toBeUndefined();expect(renderToStaticMarkup(central)).toContain('type="password"');
});
it('accepts only the closed actual Legal continuation login return and never an arbitrary callback or token query',()=>{expect(legalEnrollmentReturnPath('/legal-portal/continue?share=share&holding=h001')).toBe('/legal-portal/continue?holding=h001&share=share');for(const path of ['/legal-portal/h001','/legal-portal/continue?holding=h001&share=share&token=secret','/legal-portal/continue?holding=h001&share=a&share=b','/legal-portal/continue?holding=../foreign&share=share','https://evil.test/legal-portal/continue?holding=h001&share=share'])expect(legalEnrollmentReturnPath(path)).toBeNull();});


it('hands the exact Helpdesk ticket Portal through normal central SSO while retaining tenant, filters and fragment',async()=>{
 const target='/customer-portal/h001/tickets?binding=binding-original&tag=one&tag=two#ticket-original';
 expect(publicSurfaceTarget(target)).toEqual({host:'portal',target});
 const handoff=productNavigationHref(target,{audience:'support'}),url=new URL(handoff.href);
 expect(handoff.handoff).toBe(true);expect(url.origin).toBe(origin('portal'));expect(url.pathname).toBe('/sso/start');expect(url.searchParams.get('next')).toBe(publicSurfaceLaunchPath(target));
 expect(productNavigationHref(target,{audience:'portal'})).toEqual({href:target,handoff:false});
 const response=await publicSurfaceLaunchRequest(request('portal',target));expect(response.status).toBe(303);expect(response.headers.get('location')).toBe(origin('portal')+target);expect(response.headers.get('set-cookie')).toBeNull();
 mocks.session.mockRejectedValueOnce(Error('UNAUTHENTICATED'));const recovery=await publicSurfaceLaunchRequest(request('portal',target)),sso=new URL(recovery.headers.get('location')!);expect(sso.pathname).toBe('/sso/start');expect(sso.searchParams.get('next')).toBe(publicSurfaceLaunchPath(target));
 expect(publicSurfaceTarget('/customer-portal/h001/tickets?code=secret&token=secret&tag=one#ticket-original')).toEqual({host:'portal',target:'/customer-portal/h001/tickets?tag=one#ticket-original'});
 for(const value of ['/customer-portal/h001/tickets/other','/customer-portal/h001/tickets/../tickets','/customer-portal/%68olding/tickets','/customer-portal/h001/%74ickets','/customer-portal/h001/tickets?tag=%00'])expect(publicSurfaceTarget(value)).toBeNull();
});
