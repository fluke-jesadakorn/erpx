import {afterEach,beforeEach,describe,expect,it,vi} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {directProductEntryHref,type DirectProductEntryChoice} from '../../../packages/workplace/src/lib/product-entry';
const state=vi.hoisted(()=>({host:'service',native:vi.fn(),auth:vi.fn(),choices:vi.fn(),recovery:vi.fn(),redirect:vi.fn((path:string)=>{throw Error(`REDIRECT:${path}`);})}));
vi.mock('server-only',()=>({}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.jesadakorn.com','x-erpx-visible-path':'/service?company=acme'}),cookies:async()=>({get:()=>undefined})}));
vi.mock('next/navigation',()=>({notFound:()=>{throw Error('NOT_FOUND');},redirect:state.redirect}));
vi.mock('@erpx/feature-runtime/lib/product-hosts',()=>({productHostForHost:()=>state.host,requestProductOrigin:()=> 'https://acme.jesadakorn.com',configuredProductOrigin:()=> 'https://account.example.test',configuredProductHandoff:()=>'/apps'}));
vi.mock('@erpx/server-identity_access/public/authorization',()=>({currentSession:state.native}));
vi.mock('@erpx/server-identity_access/public/auth',()=>({createAuth:state.auth}));
vi.mock('@erpx/server-identity_access/public/account-preference-snapshot',()=>({preferredAccountLanguage:async()=> 'en'}));
vi.mock('@erpx/server-runtime/public/runtime/read-metrics',()=>({measureReadPhase:async(_name:string,run:()=>unknown)=>run()}));
vi.mock('@erpx/server-work_projection/public/product-launcher',()=>({productLaunchChoices:state.choices,existingWorkspaceLaunchChoices:state.recovery}));
vi.mock('@erpx/server-work_projection/public/admin-launcher',()=>({adminLaunchChoices:async()=>[]}));
vi.mock('@erpx/server-identity_access/public/provider-entry',()=>({providerEntryOverview:async()=>null}));
vi.mock('@erpx/server-identity_access/public/rules/page-entry-return',()=>({productEntrySsoHref:()=>'/sso/start?legacy=1'}));
vi.mock('@erpx/server-commercial/public/product-registry',()=>({product:()=>({label:{en:'Service',th:'Service'}})}));
vi.mock('@erpx/feature-work_projection/lib/product-access-label',()=>({productAccessLabel:()=>''}));
vi.mock('@erpx/workplace/components/product-navigation-boundary',()=>({ProductNavigationBoundary:({children}:{children:unknown})=>children}));
vi.mock('@erpx/workplace/components/product-link-context',()=>({AppsHomeLink:({children}:{children:unknown})=>children}));
vi.mock('@erpx/feature-brand/components/product-brand',()=>({ProductBrand:()=>null}));
vi.mock('@erpx/workplace/components/product-account-provider',()=>({ProductAccountTrigger:()=>null}));
vi.mock('@erpx/workplace/components/apps-sidebar',()=>({AppsSidebar:()=>null}));
vi.mock('@erpx/workplace/components/apps-launcher',()=>({AppsLauncher:()=>null}));
vi.mock('@erpx/feature-work_projection/lib/apps-launcher',()=>({buildAppsCompanies:()=>[]}));
vi.mock('@erpx/server-work_projection/public/workspace-entry-presentation-query',()=>({savedAppsWorkspaceForPresentation:async()=>null}));
vi.mock('@erpx/feature-identity_access/components/account-home',()=>({AccountHome:()=>null}));
vi.mock('@erpx/feature-pms/components/public-booking-directory',()=>({PublicBookingDirectory:()=>null}));
vi.mock('@erpx/ui/components/foundation',async()=>{const React=await vi.importActual<typeof import('react')>('react');const wrap=(tag:string)=>function Stub({children,...props}:{children?:React.ReactNode;[key:string]:unknown}){return React.createElement(tag,props,children);};return {PageContent:wrap('main'),Stack:wrap('div'),SectionHeader:wrap('header'),InlineNotice:wrap('div')};});
beforeEach(()=>{state.host='service';state.native.mockReset();state.auth.mockReset();state.redirect.mockClear();state.choices.mockReset().mockResolvedValue([]);state.recovery.mockReset().mockResolvedValue([]);vi.stubEnv('ERPX_HOLDING_ROUTING','enabled');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');});afterEach(()=>vi.unstubAllEnvs());
const session={session:{id:'session'},user:{id:'user',name:'Native User',email:'person@example.test'}};
describe.each(['workspace','compat'] as const)('actual %s product entry session gate',entry=>{
 const page=async()=>{for(const key of Object.keys(process.env))if(key.startsWith('ERPX_NATIVE_'))delete process.env[key];vi.resetModules();return entry==='workspace'?(await import('../../../packages/workplace/src/entries/workspace')).default({searchParams:Promise.resolve({})}):(await import('../../../packages/workplace/src/routes/product-entry/page')).default({searchParams:Promise.resolve({})});};
 it('renders after a real native-aware session result and retains downstream choices',async()=>{vi.stubEnv('ERPX_NATIVE_ACCOUNT_SESSION_CONFIG','configured');state.native.mockResolvedValue({session,requestHeaders:new Headers()});const html=renderToStaticMarkup(await page());expect(html).toContain('Choose a workspace');expect(state.native).toHaveBeenCalledOnce();expect(state.auth).not.toHaveBeenCalled();expect(state.choices).toHaveBeenCalledOnce();expect(state.recovery).toHaveBeenCalledOnce();});
 it('rejects missing native configuration without evaluating the retired credential reader',async()=>{delete process.env.ERPX_NATIVE_ACCOUNT_SESSION_CONFIG;state.native.mockRejectedValue(Error('NATIVE_SESSION_CONFIGURATION_REQUIRED'));await expect(page()).rejects.toThrow('NATIVE_SESSION_CONFIGURATION_REQUIRED');expect(state.native).toHaveBeenCalledOnce();expect(state.auth).not.toHaveBeenCalled();expect(state.choices).not.toHaveBeenCalled();expect(state.redirect).not.toHaveBeenCalled();});
 it('uses the existing sign-in redirect only for unauthenticated',async()=>{vi.stubEnv('ERPX_NATIVE_ACCOUNT_SESSION_CONFIG','configured');state.native.mockRejectedValue(Error('UNAUTHENTICATED'));await expect(page()).rejects.toThrow('REDIRECT:/sso/start?next=%2Fservice%3Fcompany%3Dacme');expect(state.auth).not.toHaveBeenCalled();expect(state.choices).not.toHaveBeenCalled();});
 it('propagates malformed or unavailable native session failures without legacy fallback',async()=>{vi.stubEnv('ERPX_NATIVE_ACCOUNT_SESSION_CONFIG','');state.native.mockRejectedValue(Error('NATIVE_SESSION_CONFIGURATION_INVALID'));await expect(page()).rejects.toThrow('NATIVE_SESSION_CONFIGURATION_INVALID');expect(state.auth).not.toHaveBeenCalled();expect(state.redirect).not.toHaveBeenCalled();expect(state.choices).not.toHaveBeenCalled();});
});
it('uses the native central session for the Account entry without Better Auth fallback',async()=>{state.host='account';vi.stubEnv('ERPX_NATIVE_ACCOUNT_SESSION_CONFIG','configured');state.native.mockResolvedValue({session,requestHeaders:new Headers()});const {default:Page}=await import('../../../packages/workplace/src/routes/product-entry/page');await Page({searchParams:Promise.resolve({})});expect(state.native).toHaveBeenCalledOnce();expect(state.auth).not.toHaveBeenCalled();expect(state.choices).not.toHaveBeenCalled();});

const financeChoice:DirectProductEntryChoice={productId:'finance',href:'https://acme.jesadakorn.com/launch?companyId=c1&app=finance&next=%2Fholdings%2Facme%2Ffinance',scope:{holdingSlug:'acme'}};
const secondChoice:DirectProductEntryChoice={...financeChoice,scope:{holdingSlug:'acme'}};
const departmentChoice={...financeChoice,href:'/holdings/acme/branches/bangkok/departments/accounts/finance',scope:{holdingSlug:'acme',kind:'department' as const,branchSlug:'bangkok',departmentSlug:'accounts'}};
const crossHoldingChoice:DirectProductEntryChoice={...financeChoice,href:'https://northstar.jesadakorn.com/launch?companyId=c2&app=finance',scope:{holdingSlug:'northstar'}};
describe.each(['workspace','compat'] as const)('actual %s single-product direct entry',entry=>{
 const page=async()=>{for(const key of Object.keys(process.env))if(key.startsWith('ERPX_NATIVE_'))delete process.env[key];vi.resetModules();return entry==='workspace'?(await import('../../../packages/workplace/src/entries/workspace')).default({searchParams:Promise.resolve({})}):(await import('../../../packages/workplace/src/routes/product-entry/page')).default({searchParams:Promise.resolve({})});};
 beforeEach(()=>{state.native.mockResolvedValue({session,requestHeaders:new Headers()});});
 it('opens the only current product workspace directly under Holding routing',async()=>{
  state.host='finance';state.choices.mockResolvedValue([financeChoice]);
  await expect(page()).rejects.toThrow(`REDIRECT:${financeChoice.href}`);
  expect(state.choices).toHaveBeenCalledOnce();
 });
 it('keeps the chooser for a second workspace or a shared Apps/Admin host',async()=>{
  state.host='finance';state.choices.mockResolvedValue([financeChoice,secondChoice]);
  renderToStaticMarkup(await page());expect(state.redirect).not.toHaveBeenCalled();
  for(const host of ['apps','admin'] as const){
   state.host=host;state.choices.mockResolvedValue([{...financeChoice,productId:host}]);
   await page();expect(state.redirect,host).not.toHaveBeenCalled();
  }
 });
 it('keeps the chooser when the owner href is not one safe current destination',async()=>{
  state.host='finance';
  for(const value of ['javascript:alert(1)','http://finance.example/sso','//evil.example/sso','https://user:secret@finance.example/sso','/','',`${financeChoice.href}\nset-cookie: forged`]){
   state.choices.mockResolvedValue([{...financeChoice,href:value}]);
   await page();expect(state.redirect,value).not.toHaveBeenCalled();
  }
  for(const choices of [[{...financeChoice,productId:'sales'}],[crossHoldingChoice],[{...financeChoice,href:'https://northstar.jesadakorn.com/launch'}]]){state.choices.mockResolvedValue(choices);await page();expect(state.redirect).not.toHaveBeenCalled();}
 });
});
it('accepts only one exact safe product choice for direct entry and never crosses Holding origin',()=>{
 expect(directProductEntryHref('finance',[financeChoice])).toBe(financeChoice.href);
 expect(directProductEntryHref('finance',[{...financeChoice,href:'/holdings/acme/finance'}])).toBe('/holdings/acme/finance');
 expect(directProductEntryHref('finance',[departmentChoice])).toBe(departmentChoice.href);
 const acme={holdingSlug:'acme',origin:'https://acme.jesadakorn.com'};
 expect(directProductEntryHref('finance',[financeChoice],acme)).toBe(financeChoice.href);
 expect(directProductEntryHref('finance',[departmentChoice],acme)).toBe(departmentChoice.href);
 expect(directProductEntryHref('finance',[crossHoldingChoice],acme)).toBeNull();
 expect(directProductEntryHref('finance',[],acme)).toBeNull();
 expect(directProductEntryHref('finance',[{...financeChoice,href:'https://northstar.jesadakorn.com/launch'}],acme)).toBeNull();
 for(const host of ['apps','admin','account','booking','portal','legal-portal','forms',''])expect(directProductEntryHref(host,[financeChoice]),host).toBeNull();
 expect(directProductEntryHref('finance',[financeChoice,secondChoice])).toBeNull();
 expect(directProductEntryHref('finance',[{...financeChoice,productId:'sales'}])).toBeNull();
 expect(directProductEntryHref('finance',[{...financeChoice,scope:{holdingSlug:''}}])).toBeNull();
 for(const href of ['javascript:alert(1)','http://finance.example/sso','//evil.example/sso','https://user:secret@finance.example/sso','/','',`${financeChoice.href}\r\nlocation: https://evil.example`])expect(directProductEntryHref('finance',[{...financeChoice,href}]),href).toBeNull();
});
