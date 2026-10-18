import {beforeEach,expect,it,vi} from 'vitest';
import {createElement as h,isValidElement,type ReactNode,type ReactElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import type {WorkAssignment} from '@/server/modules/identity_access/public/authorization';
import {workRoleDisplayChoices,platformRoleDisplayChoices} from '@/lib/identity_access/work-role-display';
vi.mock('server-only',()=>({}));
const mocks=vi.hoisted(()=>({roles:vi.fn(),management:vi.fn(),platform:vi.fn(),from:vi.fn(),switch:vi.fn(),session:vi.fn(),host:vi.fn(),provider:vi.fn(),accountOrigin:vi.fn()}));
vi.mock('@/server/modules/identity_access/public/authorization',()=>({assignmentApplies:()=>false,canEnterManageErpx:async()=>false,currentSession:mocks.session,requireManageErpxContext:mocks.management,requirePlatformContext:mocks.platform}));
vi.mock('@/server/modules/work_projection/public/work-role-options',()=>({workRoleOptions:mocks.roles}));
vi.mock('@/server/modules/organization/public/workspace-choices',()=>({workspaceChoices:async()=>[]}));
vi.mock('@/server/modules/work_projection/public/quick-access-preferences',()=>({readQuickAccessPreferences:async()=>({})}));
vi.mock('@/server/modules/work_projection/public/page-error',()=>({pageError:(error:unknown)=>{throw error;}}));
vi.mock('@/server/modules/identity_access/public/account-preference-snapshot',()=>({preferredAccountLanguage:async()=> 'en',preferredAccountAppearance:async()=> 'system'}));
vi.mock('@/components/workplace/forbidden-context',()=>({requestedPage:mocks.from,accessUnavailable:()=>false}));
vi.mock('@/components/workplace/context-switch-provider',()=>({useContextSwitch:()=>({busy:false,switchContext:mocks.switch})}));
vi.mock('@/components/workplace/context-switch-context',()=>({useContextSwitch:()=>({busy:false,switchContext:mocks.switch})}));
vi.mock('@/lib/runtime/product-hosts',()=>({productHostForHost:mocks.host,configuredProductHandoff:()=>null,configuredProductOrigin:mocks.accountOrigin}));
vi.mock('@/server/modules/identity_access/public/provider-context',()=>({currentProviderContext:mocks.provider}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:'console-dev-erpx.jesadakorn.com'}),cookies:async()=>({get:()=>undefined})}));
vi.mock('next/navigation',()=>({notFound:()=>{throw Error('NOT_FOUND');},redirect:()=>{throw Error('REDIRECT');},usePathname:()=>'/holdings/acme/work',useSearchParams:()=>new URLSearchParams(),useRouter:()=>({refresh:vi.fn(),push:vi.fn(),replace:vi.fn()})}));

const source:WorkAssignment&{href:string;permissions:string[];authority:{privateReceipt:string}}={id:'choice',version:7,label:'Finance reviewer · Branch One',role:'finance_approver',companyId:'company',branchId:'private-branch',scopeKind:'branch',scopeNodeId:'private-node',modules:['private-module'],validUntil:9999999999999,href:'/private-owner-path',permissions:['private-permission'],authority:{privateReceipt:'private-authority'}};
const platformSource:typeof source={...source,id:'platform-choice',role:'platform_admin',companyId:null,scopeKind:'platform'};
const scope={kind:'holding' as const,holdingId:'h001' as const,holdingSlug:'acme',holdingName:'Acme',companyId:'company'};
const forbidden=['private-module','private-branch','private-node','private-owner-path','private-permission','private-authority','modules','scopeNodeId','validUntil','permissions','authority'];
function assertNoPrivate(value:unknown){const text=JSON.stringify(value);for(const key of forbidden)expect(text).not.toContain(key);}
function find(node:ReactNode,name:string|ReactElement['type']):ReactElement<Record<string,unknown>>|undefined{
 if(Array.isArray(node)){for(const child of node){const value=find(child,name);if(value)return value;}return;}
 if(!isValidElement<{children?:ReactNode}>(node))return;
 if(node.type===name||typeof name==='string'&&typeof node.type==='function'&&node.type.name===name)return node as ReactElement<Record<string,unknown>>;
 return find(node.props.children,name);
}
beforeEach(()=>{vi.clearAllMocks();mocks.host.mockReturnValue(null);mocks.accountOrigin.mockReturnValue(null);mocks.provider.mockResolvedValue({accountKey:'a'.repeat(64),contextKey:'b'.repeat(64),selectedId:'platform-choice',revision:1,choices:[{id:'platform-choice',label:'Assigned Platform administrator',version:'c'.repeat(64)}]});mocks.from.mockResolvedValue('/holdings/acme/work');mocks.session.mockResolvedValue({session:{user:{id:'user',name:'User',email:'fixture@example.test'},session:{id:'session'}}});mocks.roles.mockResolvedValue({access:{scope,userId:'user',email:'fixture@example.test'},options:[source],selected:null,selection:{revision:9}});mocks.management.mockResolvedValue({platform:{userId:'user',assignmentId:'platform-choice',assignments:[platformSource]}});mocks.platform.mockResolvedValue({userId:'user',assignmentId:'platform-choice',assignments:[platformSource]});});
it('rebuilds an exact minimal allowlist while preserving selection version, Company fallback and display fields',()=>{
 const work=workRoleDisplayChoices([source]);expect(work).toEqual([{id:'choice',version:7,label:'Finance reviewer · Branch One',role:'finance_approver',companyId:'company',scopeKind:'branch'}]);assertNoPrivate(work);expect(work[0]).not.toBe(source);const platform=platformRoleDisplayChoices([platformSource]);expect(platform).toEqual([{id:'platform-choice',role:'platform_admin'}]);assertNoPrivate(platform);expect(Object.keys(platform[0])).toEqual(['id','role']);
});
it('does not manufacture optional version or label when the original legacy display choice omitted them',()=>{expect(workRoleDisplayChoices([{id:'legacy',role:'employee',scopeKind:'holding',companyId:'company'}])).toEqual([{id:'legacy',role:'employee',scopeKind:'holding',companyId:'company'}]);});
it('projects actual standalone work-role page Client props before serialization and leaves current revision and return path intact',async()=>{
 const {default:WorkRolePage}=await import('@/app/work-role/page'),page=await WorkRolePage({params:Promise.resolve({}),searchParams:Promise.resolve({from:'/holdings/acme/work?status=waiting#draft'})}),picker=find(page,'WorkRolePicker');expect(picker).toBeDefined();expect(picker!.props.options).toEqual(workRoleDisplayChoices([source]));expect(picker!.props.revision).toBe(9);expect(picker!.props.from).toBe('/holdings/acme/work?status=waiting#draft');expect(picker!.props.companyId).toBe('company');assertNoPrivate(picker!.props);expect(mocks.switch).not.toHaveBeenCalled();
});
it('projects the actual forbidden recovery AccountSwitcher choices before the Client boundary',async()=>{
 const {ForbiddenRecoveryShell}=await import('@/components/workplace/forbidden-recovery'),page=await ForbiddenRecoveryShell(),menu=find(page,'AccountSwitcher');expect(menu).toBeDefined();expect(menu!.props.assignments).toEqual(workRoleDisplayChoices([source]));expect(menu!.props.contextRevision).toBe(9);expect(menu!.props.companyId).toBe('company');assertNoPrivate(menu!.props);expect(mocks.switch).not.toHaveBeenCalled();
});
it('projects actual legacy Management Platform choices before the Client boundary without serializing Company modules or grants',async()=>{
 const {default:ManageErpxLayout}=await import('@/app/holdings/[holdingSlug]/_presenters/manage-erpx/layout'),page=await ManageErpxLayout({params:Promise.resolve({holdingSlug:'erpx'}),children:'Original Management body'}),picker=find(page,'PlatformRoleSwitcher');expect(picker).toBeDefined();expect(picker!.props.assignments).toEqual(platformRoleDisplayChoices([platformSource]));expect(picker!.props.activeAssignmentId).toBe('platform-choice');assertNoPrivate(picker!.props);expect(mocks.switch).not.toHaveBeenCalled();
});
it('projects the actual Platform forbidden recovery picker and keeps normal independent server authorization',async()=>{
 mocks.from.mockResolvedValue('/holdings/erpx/manage-erpx/iam');const {ForbiddenPanel}=await import('@/components/workplace/forbidden-panel'),panel=await ForbiddenPanel(),recovery=find(panel,'PlatformRoleRecovery');expect(recovery).toBeDefined();const render=recovery!.type as (props:{from:string|null})=>Promise<ReactElement>;const page=await render({from:'/holdings/erpx/manage-erpx/iam'}),picker=find(page,(await import('@/components/workplace/platform-role-switcher-loader')).PlatformRoleSwitcher);expect(picker).toBeDefined();expect(picker!.props.assignments).toEqual(platformRoleDisplayChoices([platformSource]));assertNoPrivate(picker!.props);expect(mocks.platform).toHaveBeenCalledTimes(1);expect(mocks.switch).not.toHaveBeenCalled();
});
it('retains actual role and Platform SSR labels and selected IDs using only minimal display choices',async()=>{
 const {RoleSelect}=await import('@/components/workplace/role-select'),{PlatformRoleSwitcher}=await import('@/components/workplace/platform-role-switcher'),work=renderToStaticMarkup(h(RoleSelect,{assignments:workRoleDisplayChoices([source]),value:'choice',disabled:true,onChange:vi.fn()})),platform=renderToStaticMarkup(h(PlatformRoleSwitcher,{assignments:platformRoleDisplayChoices([platformSource]),activeAssignmentId:'platform-choice'}));expect(work).toContain('Finance reviewer');expect(work).toContain('Branch One');expect(platform).toContain('platform admin');expect(platform).toContain('value="platform-choice"');for(const key of forbidden){expect(work).not.toContain(key);expect(platform).not.toContain(key);}expect(mocks.switch).not.toHaveBeenCalled();
});
it('uses the dedicated minimal Provider switcher on actual Admin Management without serializing legacy actor or performing a mount probe',async()=>{
 mocks.host.mockReturnValue('admin');const {default:ManageErpxLayout}=await import('@/app/holdings/[holdingSlug]/_presenters/manage-erpx/layout'),page=await ManageErpxLayout({params:Promise.resolve({holdingSlug:'erpx'}),children:'Original Management body'}),boundary=find(page,'ProviderContextBoundary');expect(boundary).toBeDefined();expect(find(page,'ProviderContextSwitcher')).toBeDefined();expect(find(page,'PlatformRoleSwitcher')).toBeUndefined();expect(find(page,'PlatformContextBoundary')).toBeUndefined();expect(boundary!.props.initial).toEqual(await mocks.provider.mock.results[0].value);assertNoPrivate(boundary!.props.initial);expect(Object.keys(boundary!.props.initial as object).sort()).toEqual(['accountKey','choices','contextKey','revision','selectedId']);expect(mocks.management).toHaveBeenCalledTimes(1);expect(mocks.provider).toHaveBeenCalledTimes(1);expect(mocks.switch).not.toHaveBeenCalled();
});
it('keeps original dual Company and Platform authority before reading a Provider seed for Admin Management',async()=>{
 mocks.host.mockReturnValue('admin');mocks.management.mockRejectedValueOnce(Error('ORIGINAL_MANAGEMENT_AUTHORITY_REQUIRED'));const {default:ManageErpxLayout}=await import('@/app/holdings/[holdingSlug]/_presenters/manage-erpx/layout');await expect(ManageErpxLayout({params:Promise.resolve({holdingSlug:'erpx'}),children:'Original Management body'})).rejects.toThrow('ORIGINAL_MANAGEMENT_AUTHORITY_REQUIRED');expect(mocks.provider).not.toHaveBeenCalled();expect(mocks.switch).not.toHaveBeenCalled();
});
it('rejects a concurrent Provider selection between original Management authority and the Server seed',async()=>{
 mocks.host.mockReturnValue('admin');mocks.provider.mockResolvedValueOnce({accountKey:'a'.repeat(64),contextKey:'b'.repeat(64),selectedId:'other-assigned-platform-role',revision:2,choices:[]});const {default:ManageErpxLayout}=await import('@/app/holdings/[holdingSlug]/_presenters/manage-erpx/layout');await expect(ManageErpxLayout({params:Promise.resolve({holdingSlug:'erpx'}),children:'Original Management body'})).rejects.toMatchObject({code:'PLATFORM_CONTEXT_CHANGED',status:409});expect(mocks.switch).not.toHaveBeenCalled();
});

it('passes only the configured Account navigation availability flag to actual forbidden recovery while retaining minimal role display props',async()=>{mocks.accountOrigin.mockReturnValue('https://account-dev-erpx.jesadakorn.com');const {ForbiddenRecoveryShell}=await import('@/components/workplace/forbidden-recovery'),page=await ForbiddenRecoveryShell(),menu=find(page,'AccountSwitcher');expect(menu).toBeDefined();expect(menu!.props.accountNavigation).toBe(true);expect(menu!.props.assignments).toEqual(workRoleDisplayChoices([source]));assertNoPrivate(menu!.props);expect(mocks.switch).not.toHaveBeenCalled();});
