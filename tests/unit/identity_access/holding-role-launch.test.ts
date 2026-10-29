import {afterEach,describe,expect,it,vi} from 'vitest';
import {renderToStaticMarkup} from 'react-dom/server';
import {holdingGatewayApp} from '../../../scripts/local/holding-gateway';
const fixture=vi.hoisted(()=>({audience:'service'}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'acme.jesadakorn.com'}),cookies:async()=>({get:()=>undefined})}));
vi.mock('next/navigation',()=>({notFound:()=>{throw Error('not found');},redirect:()=>{throw Error('unexpected redirect');}}));
vi.mock('@erpx/feature-runtime/lib/product-hosts',()=>({productHostForHost:()=>fixture.audience,configuredProductHandoff:()=>'/'}));
vi.mock('@erpx/server-commercial/public/product-registry',()=>({product:()=>({label:{en:'Product',th:'Product'}})}));
vi.mock('@erpx/server-identity_access/public/account-preference-snapshot',()=>({preferredAccountLanguage:async()=> 'en'}));
vi.mock('@erpx/server-identity_access/public/product-workspace',()=>({productWorkspaceTarget:()=>({companyId:'company',target:'/service/order'}),productLaunchPath:()=>'/launch',productWorkRoleChoices:async()=>({company:{name:'Company'},revision:7,options:[{assignmentId:'grant',assignmentVersion:3,label:'Worker'}]})}));
vi.mock('@erpx/feature-work_projection/lib/product-access-label',()=>({productAccessLabel:()=>''}));
vi.mock('@erpx/workplace/components/product-navigation-boundary',()=>({ProductNavigationBoundary:({children}:{children:unknown})=>children}));
vi.mock('@erpx/workplace/components/product-link-context',()=>({AppsHomeLink:({children}:{children:unknown})=>children}));
vi.mock('@erpx/feature-brand/components/product-brand',()=>({ProductBrand:()=>null}));
vi.mock('@erpx/ui/components/foundation',async()=>{const React=await vi.importActual<typeof import('react')>('react');const wrap=(tag:string)=>function Stub({children,...props}:{children?:React.ReactNode;[key:string]:unknown}){return React.createElement(tag,props,children);};return {Button:wrap('button'),Card:wrap('div'),PageContent:wrap('main'),Stack:wrap('div'),InlineNotice:wrap('div')};});
afterEach(()=>vi.unstubAllEnvs());
describe('actual Holding role chooser form transport',()=>{
 it.each(['service','finance'])('routes the rendered %s action to its real app and preserves selection fields',async app=>{
  fixture.audience=app;vi.stubEnv('ERPX_HOLDING_ROUTING','enabled');vi.stubEnv('ERPX_ENVIRONMENT','local');vi.stubEnv('ERPX_PRODUCT_ENVIRONMENT','development');const {default:Page}=await import('../../../packages/workplace/src/routes/choose-work-role/page');const html=renderToStaticMarkup(await Page({searchParams:Promise.resolve({})}));const action=html.match(/<form[^>]*action="([^"]+)"/)?.[1];expect(action).toBe(`/launch?app=${app}`);expect(holdingGatewayApp('acme.jesadakorn.com',action!)).toBe(app);for(const [name,value] of [['companyId','company'],['next','/service/order'],['assignmentId','grant'],['assignmentVersion','3'],['revision','7']])expect(html).toContain(`name="${name}" value="${value}"`);
 });
 it('keeps the legacy-host action unchanged',async()=>{fixture.audience='service';vi.stubEnv('ERPX_HOLDING_ROUTING','disabled');const {default:Page}=await import('../../../packages/workplace/src/routes/choose-work-role/page');const html=renderToStaticMarkup(await Page({searchParams:Promise.resolve({})}));expect(html.match(/<form[^>]*action="([^"]+)"/)?.[1]).toBe('/launch');});
});
