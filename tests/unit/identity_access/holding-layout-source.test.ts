import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {cloneElement,createElement as h,isValidElement,type ComponentProps,type ReactElement,type ReactNode} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import type {WorkAssignment,WorkContext} from '@erpx/server-identity_access/public/authorization';
vi.mock('server-only',()=>({}));
const state=vi.hoisted(()=>({host:'finance-dev-erpx.jesadakorn.com',path:'/holdings/acme/finance/accounting/records/journal/included/overview',language:'en' as 'en'|'th'}));
const reads=vi.hoisted(()=>({actor:vi.fn(),access:vi.fn(),restricted:vi.fn(),navigation:vi.fn(),money:vi.fn(),companies:vi.fn(),admin:vi.fn(),executive:vi.fn(),handbook:vi.fn(),catalogue:vi.fn(),identity:vi.fn(),brand:vi.fn(),platform:vi.fn(),workspaceChoices:vi.fn(),manage:vi.fn(),products:vi.fn(),adminProducts:vi.fn(),presentation:vi.fn(),collaboration:vi.fn(),account:vi.fn(),adminAccess:vi.fn()}));
vi.mock('next/headers',()=>({headers:async()=>new Headers({host:state.host,'x-erpx-page-path':state.path}),cookies:async()=>({get:(key:string)=>key==='erpx_language'?{value:state.language}:undefined})}));
vi.mock('next/navigation',()=>({usePathname:()=>new URL(state.path,'https://entry.invalid').pathname,useSearchParams:()=>new URL(state.path,'https://entry.invalid').searchParams,useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn(),prefetch:vi.fn()}),redirect:(path:string)=>{throw Error(`REDIRECT:${path}`);},notFound:()=>{throw Error('NOT_FOUND');},unstable_rethrow:()=>{}}));
vi.mock("@erpx/server-identity_access/public/authorization",()=>({requireWorkContext:reads.actor,publicScope:(actor:WorkContext)=>actor.scope,requirePlatformContext:reads.platform,canEnterManageErpx:reads.manage,requireAccountAccess:reads.account}));
vi.mock("@erpx/server-identity_access/public/product-workspace",()=>({productPageAccess:reads.access}));
vi.mock("@erpx/server-identity_access/public/admin-workspace",()=>({adminPageAccess:reads.adminAccess,adminWorkspaceTarget:vi.fn(),adminWorkRolePath:vi.fn()}));
vi.mock("@erpx/server-work_projection/public/navigation-snapshot",()=>({workspaceNavigation:reads.navigation,restrictedWorkspaceNavigation:reads.restricted}));
vi.mock("@erpx/server-work_projection/public/presentation-queries",()=>({workProjectionPresentationQueries:reads.presentation}));
vi.mock("@erpx/server-collaboration/public/presentation-queries",()=>({collaborationPresentationQueries:reads.collaboration}));
vi.mock("@erpx/server-identity_access/public/workspace-company-presentation",()=>({listWorkspaceCompanyPresentationFacts:reads.companies}));
vi.mock("@erpx/server-finance/public/company-money",()=>({moneyContext:reads.money}));
vi.mock("@erpx/server-finance/public/finance-presentation",()=>({financeWorkspaceV2Enabled:()=>true}));
vi.mock("@erpx/server-runtime/public/administration",()=>({administrationAccess:reads.admin}));
vi.mock("@erpx/server-pms/public/hospitality-rollout",()=>({companyFeatureCatalogue:reads.catalogue}));
vi.mock("@erpx/server-organization/public/workspace-choices",()=>({workspaceChoices:reads.workspaceChoices}));
vi.mock("@erpx/server-work_projection/public/product-launcher",()=>({productLaunchChoices:reads.products}));
vi.mock("@erpx/server-work_projection/public/admin-launcher",()=>({adminLaunchChoices:reads.adminProducts}));
vi.mock("@erpx/server-work_projection/public/home-cache",()=>({homeContextKey:()=> 'opaque-render-context'}));
vi.mock("@erpx/server-work_projection/public/page-error",()=>({pageError:(error:unknown)=>{throw error;}}));
vi.mock("@erpx/server-runtime/public/runtime/read-metrics",()=>({measureReadPhase:async<Result>(_name:string,read:()=>Promise<Result>)=>read()}));
vi.mock("@erpx/server-identity_access/public/account-preference-snapshot",()=>({preferredAccountLanguage:async()=>state.language,preferredAccountAppearance:async()=> 'system'}));
vi.mock("@erpx/feature-runtime/lib/product-hosts",()=>({productHostForHost:(host:string)=>host.startsWith('finance-')?'finance':host.startsWith('work-')?'work':host.startsWith('sales-')?'sales':host.startsWith('admin-')?'admin':null,configuredProductOrigin:(key:string)=>`https://${key}-dev-erpx.jesadakorn.com`,configuredProductHandoff:(key:string)=>`https://${key}-dev-erpx.jesadakorn.com/`,configuredDevelopmentProductOrigin:(key:string)=>`https://${key}-dev-erpx.jesadakorn.com`}));

import WorkplaceLayout from '@erpx/workplace/routes/holdings/[holdingSlug]/layout';
import {OfficeProductMenus} from '@erpx/workplace/components/office-product-menus';
import {FeatureEnvironment} from '@erpx/feature-shared/components/features/feature-environment';
import {ConsoleAccountSwitcher,ConsoleCommandSearch,ConsoleWorkplaceNavigation} from '@erpx/workplace/components/console-ui-utilities';
import {NavigationCountsProvider} from '@erpx/workplace/components/navigation-counts';
import {WorkspaceAssistant} from '@erpx/workplace/components/workspace-assistant';
import {WorkspaceBreadcrumb} from '@erpx/workplace/components/workspace-breadcrumb';
import {CurrencyProvider} from '@erpx/feature-finance/components/money/currency-provider';
import {ContextSwitchContext} from '@erpx/workplace/components/context-switch-context';
import {FinancePurchaseProvider} from '@erpx/feature-finance/components/purchase-context';
import {JournalAction} from '@erpx/feature-finance/components/ledger-controls';
import {workRoleDisplayChoices} from '@erpx/feature-identity_access/lib/work-role-display';
import WorkspaceLink from '@erpx/workplace/components/workspace-link';

const privateChoice:WorkAssignment&{authority:{privateNativeSource:string};grants:string[]}={id:'assigned-role',version:4,label:'Existing source reviewer',role:'holding_owner',companyId:'company',branchId:'private-branch-marker',scopeKind:'holding',scopeNodeId:'private-scope-marker',modules:['private-module-marker'],validUntil:9999999999999,authority:{privateNativeSource:'private-native-source-marker'},grants:['private-grant-marker']};
function actor():WorkContext{return {holdingId:'h003',holdingSlug:'acme',holdingName:'Actual Holding',userId:'current-user',name:'Source reviewer',email:'reviewer@example.test',companyId:'company',branchId:null,departmentId:null,assignmentId:'assigned-role',role:'holding_owner',assignmentScope:'holding',inherited:false,scope:{kind:'holding',holdingId:'h003',holdingSlug:'acme',holdingName:'Actual Holding',companyId:'company'},modules:['finance','private-module-marker'],assignments:[privateChoice],availableAssignments:[privateChoice],workspaces:[],authority:{sessionId:'private-session-marker',revision:8,assignmentVersion:4},iam:{identityVersion:3,roleRevision:4,permissions:['manage_erpx.access','ai.context.cross','private-permission-marker'],grantablePermissions:['private-grantable-marker'],principalId:'private-principal-marker',personId:null,freshUntil:null}};}
const discoveries=[reads.navigation,reads.companies,reads.admin,reads.executive,reads.handbook,reads.catalogue,reads.platform,reads.workspaceChoices,reads.manage,reads.products,reads.adminProducts,reads.collaboration];
const forbidDiscovery=()=>{throw Error('SOURCE_ONLY_DISCOVERY_READ');};
beforeEach(()=>{
 vi.resetAllMocks();vi.stubEnv('NODE_ENV','production');state.host='finance-dev-erpx.jesadakorn.com';state.path='/holdings/acme/finance/accounting/records/journal/included/overview';state.language='en';
 const context=actor();reads.actor.mockResolvedValue(context);reads.restricted.mockResolvedValue({context,initialNavigation:undefined});
 reads.access.mockImplementation(async()=>({sourceRecord:true,target:state.path,chooseRole:'/choose-work-role'}));
 for(const read of discoveries)read.mockImplementation(forbidDiscovery);
 reads.identity.mockResolvedValue({slug:'actual-company',display_name:'Actual Company'});reads.brand.mockResolvedValue({label:'Actual Company',initials:'AC',primaryUrl:null});
 reads.presentation.mockResolvedValue({activeCompanyIdentity:reads.identity,resolveWorkspaceBrand:reads.brand,executiveVisible:reads.executive});
 reads.money.mockResolvedValue({companyId:'company',defaultCurrency:'THB',displayCurrency:'THB',scales:{THB:2},rates:[],currencies:[{code:'THB',name:'Thai Baht',scale:2}],canConfirmRate:false});
 vi.stubGlobal('fetch',vi.fn(()=>{throw Error('UNEXPECTED_SSR_FETCH');}));
});
afterEach(()=>{vi.unstubAllEnvs();vi.unstubAllGlobals();});

function find<Props>(node:ReactNode,type:ReactElement['type'],matches:(props:Props)=>boolean=()=>true):ReactElement<Props>|undefined{
 if(Array.isArray(node)){for(const child of node){const found=find<Props>(child,type,matches);if(found)return found;}return;}
 if(isValidElement<Props>(node)&&node.type===type&&matches(node.props))return node;
 return isValidElement<{children?:ReactNode}>(node)?find<Props>(node.props.children,type,matches):undefined;
}
function renderedProps(node:ReactNode):unknown[]{
 if(Array.isArray(node))return node.flatMap(renderedProps);
 if(!isValidElement<{children?:ReactNode}>(node))return [];
 const {children,...props}=node.props;return [props,...renderedProps(children)];
}
/** Resolve this actual nested async Server Component, rather than accepting an
 * unexecuted JSX reference that could conceal its discovery reads from SSR. */
async function resolveProductMenus(node:ReactNode):Promise<ReactNode>{
 if(Array.isArray(node)){const resolved=await Promise.all(node.map(resolveProductMenus));return resolved.every((child,index)=>child===node[index])?node:resolved;}
 if(isValidElement<ComponentProps<typeof OfficeProductMenus>>(node)&&node.type===OfficeProductMenus)return resolveProductMenus(await OfficeProductMenus(node.props));
 if(!isValidElement<{children?:ReactNode}>(node)||node.props.children===undefined)return node;
 const children=await resolveProductMenus(node.props.children);if(children===node.props.children)return node;
 return Array.isArray(children)?cloneElement(node,{},...children):cloneElement(node,{},children);
}
async function layout(children:ReactNode=h('h1',null,'Owner record initial content')){return WorkplaceLayout({params:Promise.resolve({holdingSlug:'acme'}),children,record:h('p',{'data-owner-record':true},'Original record drawer content')});}
function assertRestricted(page:ReactNode){
 for(const type of [OfficeProductMenus,ConsoleCommandSearch,ConsoleWorkplaceNavigation,NavigationCountsProvider,WorkspaceAssistant,WorkspaceBreadcrumb])expect(find(page,type)).toBeUndefined();
 const feature=find<ComponentProps<typeof FeatureEnvironment>>(page,FeatureEnvironment);expect(feature).toBeDefined();
 expect(feature!.props.navigation).toMatchObject({moduleIds:[],permissions:[],adminSections:[],canManageApprovals:false,manageErpxVisible:false,executiveVisible:false,handbookAccess:{editor:false,owner:false},availableAppIds:[]});
 expect(feature!.props.productionCatalogue).toBeUndefined();
 const account=find<ComponentProps<typeof ConsoleAccountSwitcher>>(page,ConsoleAccountSwitcher);expect(account).toBeDefined();
 expect(Object.keys(account!.props).sort()).toEqual(['accountNavigation','activeUser','assignments','companyId','contextRevision','preferredAssignmentId','theme','workspaceBase']);
 expect(account!.props.assignments).toEqual(workRoleDisplayChoices([privateChoice]));expect(Object.keys(account!.props.assignments![0]).sort()).toEqual(['companyId','id','label','role','scopeKind','version']);
 expect(account!.props).toMatchObject({preferredAssignmentId:'assigned-role',contextRevision:8,companyId:'company',accountNavigation:true,activeUser:{id:'current-user',name:'Source reviewer',email:'reviewer@example.test'}});
 expect(find<ComponentProps<typeof CurrencyProvider>>(page,CurrencyProvider)!.props.initial).toMatchObject({companyId:'company',canConfirmRate:false});
 const encoded=JSON.stringify(renderedProps(page));for(const marker of ['private-module-marker','private-native-source-marker','private-grant-marker','private-branch-marker','private-scope-marker','private-session-marker','private-permission-marker','private-principal-marker','private-grantable-marker'])expect(encoded).not.toContain(marker);
}
async function sourceMarkup(page:ReactNode){return renderToStaticMarkup(h(ContextSwitchContext,{value:{busy:false,switchContext:async()=>false}},await resolveProductMenus(page)));}

it.each([
 {name:'Sales posting setup',host:'sales',path:'customers-sales/sales/settings/posting',label:'Sales posting setup',language:'en'},
 {name:'Sales posting setup in Thai',host:'sales',path:'customers-sales/sales/settings/posting',label:'ตั้งค่าบัญชีงานขาย',language:'th'},
 {name:'included Finance journal',host:'finance',path:'finance/accounting/records/journal/included/overview',label:'Included Finance work',language:'en'},
 {name:'included Project task on Work',host:'work',path:'projects-routines/tasks/actual-project-task/overview',label:'Project task',language:'en'},
 {name:'source notification on Work',host:'work',path:'inbox/notifications',label:'Notifications',language:'en'},
 {name:'included published conversation on Work',host:'work',path:'posts/actual-source-post',label:'บทสนทนาของรายการต้นทาง',language:'th'},
 {name:'original personal request on Work',host:'work',path:'leave',label:'My requests',language:'en'},
] as const)('renders actual $name initial owner content without any workspace discovery, counts or Client authority snapshots',async entry=>{
 state.host=`${entry.host}-dev-erpx.jesadakorn.com`;state.path=`/holdings/acme/${entry.path}`;state.language=entry.language;
 const page=await layout();assertRestricted(page);const markup=await sourceMarkup(page);
 const recoveryLabel=entry.language==='th'?'เลือกผลิตภัณฑ์ใน Apps':'Choose a product in Apps',recovery=find<ComponentProps<typeof WorkspaceLink>>(page,WorkspaceLink,props=>props['aria-label']===recoveryLabel);
 expect(recovery).toBeDefined();expect(recovery!.props.href).toBe('https://apps-dev-erpx.jesadakorn.com/');expect(markup).toContain(`aria-label="${recoveryLabel}"`);
 expect(markup).toContain('Owner record initial content');expect(markup).toContain('Original record drawer content');expect(markup).toContain(entry.label);expect(markup).toContain('Actual Company');expect(markup).toContain('https://apps-dev-erpx.jesadakorn.com/');
 for(const read of discoveries)expect(read).not.toHaveBeenCalled();expect(reads.restricted).toHaveBeenCalledExactlyOnceWith('acme');expect(reads.identity).toHaveBeenCalledExactlyOnceWith('company');expect(reads.brand).toHaveBeenCalledTimes(1);expect(reads.money).toHaveBeenCalledTimes(1);expect(fetch).not.toHaveBeenCalled();
});

it('keeps the actual included journal posting control inside the restricted owner layout without exposing generic Finance navigation',async()=>{
 const children=h(FinancePurchaseProvider,{purchases:{'finance.accounting':{newWork:false,settleExisting:false,history:false,revision:'opaque-source-revision'}},sourceHistory:true,children:h(JournalAction,{base:'/holdings/acme',journal:{id:'included',status:'draft',source_type:'stock_receipt',version:1},canPost:true,sourceCanPost:true,periods:[]})}),page=await layout(children);
 assertRestricted(page);const markup=await sourceMarkup(page);expect(markup).toContain('Approve and post journal');expect(markup).not.toContain('Create journal');for(const read of discoveries)expect(read).not.toHaveBeenCalled();expect(fetch).not.toHaveBeenCalled();
});

it('retains the original full-workspace discovery and server seed outside source-only entry',async()=>{
 const context=actor(),initial=Promise.resolve({home:2,work:1,workViewAvailability:{team:true,routing:true}});reads.access.mockImplementation(async()=>({sourceRecord:false,target:state.path,chooseRole:'/choose-work-role'}));
 reads.navigation.mockResolvedValue({context,initialNavigation:initial});reads.companies.mockResolvedValue([{id:'company',name:'Actual Company',slug:'actual-company'}]);reads.admin.mockResolvedValue({sections:['organization'],manageOrganization:true});reads.executive.mockResolvedValue(true);reads.handbook.mockResolvedValue({editor:true,owner:true});reads.collaboration.mockResolvedValue({handbookAccess:reads.handbook});reads.platform.mockResolvedValue({userId:context.userId});reads.catalogue.mockResolvedValue([{id:'finance',name:'Finance',state:'Available'}]);reads.products.mockResolvedValue([]);reads.adminProducts.mockResolvedValue([]);
 const page=await layout();await resolveProductMenus(page);
 for(const read of [reads.navigation,reads.companies,reads.admin,reads.executive,reads.handbook,reads.catalogue,reads.platform,reads.products,reads.adminProducts])expect(read).toHaveBeenCalledTimes(1);
 expect(reads.restricted).not.toHaveBeenCalled();expect(reads.identity).not.toHaveBeenCalled();
 const feature=find<ComponentProps<typeof FeatureEnvironment>>(page,FeatureEnvironment),counts=find<ComponentProps<typeof NavigationCountsProvider>>(page,NavigationCountsProvider);
 expect(feature!.props.productionCatalogue).toEqual([{id:'finance',name:'Finance',state:'Available'}]);expect(feature!.props.navigation).toMatchObject({moduleIds:context.modules,permissions:context.iam!.permissions,canManageApprovals:true,executiveVisible:true,manageErpxVisible:true,handbookAccess:{editor:true,owner:true}});expect(counts!.props.initial).toBe(initial);
 expect(find(page,ConsoleCommandSearch)).toBeDefined();expect(find(page,ConsoleWorkplaceNavigation)).toBeDefined();expect(find(page,WorkspaceAssistant)).toBeDefined();expect(fetch).not.toHaveBeenCalled();
});


// Actual async layout and Client breadcrumb regression. The owner readers below
// remain explicit test ports; these are navigation/render tests, not IAM proof.
import {readFileSync as breadcrumbRead} from 'node:fs';
import {createRequire as breadcrumbRequire} from 'node:module';
import {resolve as breadcrumbResolve} from 'node:path';
import {Button as BreadcrumbButton} from '@erpx/ui/components/foundation';
import {ChevronRight as BreadcrumbChevron,MoreHorizontal as BreadcrumbMore} from 'lucide-react';
import {navigationTrail as breadcrumbTrail,workplaceNavigation as breadcrumbNavigation} from '@erpx/feature-work_projection/lib/workplace-navigation';

const breadcrumbHolding='/holdings/acme',breadcrumbBranch=breadcrumbHolding+'/branches/north',breadcrumbDepartment=breadcrumbBranch+'/departments/design';
function breadcrumbContext(allowed:readonly string[]=[breadcrumbHolding,breadcrumbBranch,breadcrumbDepartment]):WorkContext{
 const original=actor();
 return {...original,branchId:'north-id',departmentId:'design-id',inherited:true,scope:{kind:'department',holdingId:original.holdingId,holdingSlug:'acme',holdingName:original.holdingName,companyId:original.companyId,branchId:'north-id',branchSlug:'north',branchName:'North Branch',departmentId:'design-id',departmentSlug:'design',departmentName:'Design Department'},workspaces:allowed.map((base,index)=>({key:`native-workspace-${index}`,href:base+'/home',label:base,kind:base===breadcrumbHolding?'holding':base===breadcrumbBranch?'branch':'department',inherited:base!==breadcrumbDepartment}))};
}
function useBreadcrumbWorkspace(context:WorkContext){
 reads.actor.mockResolvedValue(context);reads.account.mockResolvedValue({scope:context.scope});reads.adminAccess.mockImplementation(async()=>({target:state.path,chooseRole:'/admin/choose-work-role'}));reads.access.mockImplementation(async()=>({sourceRecord:false,target:state.path,chooseRole:'/choose-work-role'}));
 reads.navigation.mockResolvedValue({context,initialNavigation:Promise.resolve({home:0,work:0})});reads.companies.mockResolvedValue([{id:context.companyId,name:'Actual Company',slug:'actual-company'}]);reads.admin.mockResolvedValue({sections:['organization'],manageOrganization:true});reads.executive.mockResolvedValue(false);reads.handbook.mockResolvedValue({editor:false,owner:false});reads.collaboration.mockResolvedValue({handbookAccess:reads.handbook});reads.platform.mockResolvedValue({userId:context.userId});reads.catalogue.mockResolvedValue([]);
}
function layoutBreadcrumb(page:ReactNode){
 const crumb=find<ComponentProps<typeof WorkspaceBreadcrumb>>(page,WorkspaceBreadcrumb);expect(crumb).toBeDefined();if(!crumb)throw Error('Actual layout did not render its breadcrumb');return crumb.props;
}
function breadcrumbMarkup(props:ComponentProps<typeof WorkspaceBreadcrumb>){return renderToStaticMarkup(h(ContextSwitchContext,{value:{busy:false,switchContext:async()=>false}},h(WorkspaceBreadcrumb,props)));}
function breadcrumbAnchors(markup:string){return [...markup.matchAll(/<a\b([^>]*)>/g)].map(match=>({href:/\bhref="([^"]*)"/.exec(match[1])?.[1],fullNavigation:/\bdata-product-switch(?:=|\s|$)/.test(match[1])}));}

it.each([
 {host:'admin',route:'administration/technology/directory/objects',landing:'administration'},
 {host:'finance',route:'finance/accounting/journals',landing:'finance/documents'},
 {host:'work',route:'calendar/mine',landing:'home'},
 {host:'console',route:'administration/technology/directory/objects',landing:'home'},
] as const)('keeps authorized $host ancestors in the current owner landing with document navigation',async entry=>{
 state.host=`${entry.host}-dev-erpx.jesadakorn.com`;state.path=`${breadcrumbDepartment}/${entry.route}`;const context=breadcrumbContext();useBreadcrumbWorkspace(context);
 const page=await layout(),props=layoutBreadcrumb(page);expect(props.workspaceBase).toBe(breadcrumbDepartment);expect(props.ancestors).toEqual([
  {label:'Actual Holding',href:`${breadcrumbHolding}/${entry.landing}`,fullNavigation:true},
  {label:'North Branch',href:`${breadcrumbBranch}/${entry.landing}`,fullNavigation:true},
  {label:'Design Department',href:`${breadcrumbDepartment}/${entry.landing}`,fullNavigation:false},
 ]);
 const anchors=breadcrumbAnchors(breadcrumbMarkup(props));for(const base of [breadcrumbHolding,breadcrumbBranch])expect(anchors).toContainEqual({href:`${base}/${entry.landing}`,fullNavigation:true});
 if(entry.host==='admin'){expect(reads.account).toHaveBeenCalledExactlyOnceWith('acme');expect(reads.adminAccess).toHaveBeenCalledExactlyOnceWith(context.companyId,state.path);expect(reads.access).not.toHaveBeenCalled();}
 else if(entry.host==='console'){expect(reads.account).not.toHaveBeenCalled();expect(reads.adminAccess).not.toHaveBeenCalled();expect(reads.access).not.toHaveBeenCalled();}
 else expect(reads.access).toHaveBeenCalledExactlyOnceWith(entry.host,context.companyId,state.path);
 expect(reads.navigation).toHaveBeenCalledExactlyOnceWith('acme');expect(reads.restricted).not.toHaveBeenCalled();expect(fetch).not.toHaveBeenCalled();
});

it('keeps ancestor labels without links when the original workspace allowlist omits their home entries',async()=>{
 state.host='admin-dev-erpx.jesadakorn.com';state.path=breadcrumbDepartment+'/administration/technology/directory/objects';const context=breadcrumbContext([breadcrumbDepartment]);
 // A product-specific path is not a substitute for the existing native home
 // allowlist entry. The fix changes the destination, not link authorization.
 context.workspaces.push({key:'not-a-home-allowance',href:breadcrumbHolding+'/administration',label:'Not a native home entry',kind:'holding',inherited:true});useBreadcrumbWorkspace(context);
 const props=layoutBreadcrumb(await layout());expect(props.ancestors).toEqual([{label:'Actual Holding',fullNavigation:true},{label:'North Branch',fullNavigation:true},{label:'Design Department',href:breadcrumbDepartment+'/administration',fullNavigation:false}]);
 const markup=breadcrumbMarkup(props),anchors=breadcrumbAnchors(markup);expect(markup).toContain('Actual Holding');expect(markup).toContain('North Branch');expect(anchors.some(anchor=>anchor.href===breadcrumbHolding+'/administration'||anchor.href===breadcrumbBranch+'/administration')).toBe(false);expect(fetch).not.toHaveBeenCalled();
});

it('uses the ERPX Management root for the actual Admin management layout instead of Company administration or home',async()=>{
 state.host='admin-dev-erpx.jesadakorn.com';const managementBase='/holdings/erpx';state.path=managementBase+'/erpx-management/usage';const context=actor();context.holdingSlug='erpx';if(context.scope.kind==='holding')context.scope.holdingSlug='erpx';context.workspaces=[{key:'native-holding',href:managementBase+'/home',label:'Actual Holding',kind:'holding',inherited:false}];useBreadcrumbWorkspace(context);
 const props=layoutBreadcrumb(await WorkplaceLayout({params:Promise.resolve({holdingSlug:'erpx'}),children:h('h1',null,'Management content'),record:null}));expect(props.workspaceBase).toBe(managementBase);expect(props.ancestors).toEqual([{label:'Actual Holding',href:managementBase+'/erpx-management',fullNavigation:false}]);const anchors=breadcrumbAnchors(breadcrumbMarkup(props));expect(anchors.some(anchor=>anchor.href===managementBase+'/home'||anchor.href===managementBase+'/administration')).toBe(false);expect(reads.adminAccess).toHaveBeenCalledExactlyOnceWith(context.companyId,state.path);expect(fetch).not.toHaveBeenCalled();
});

/** Execute the actual breadcrumb declaration with a controlled local React
 * state port to click its real overflow trigger. Its navigation functions,
 * Links, Button and rendered markup remain the real implementations. This is
 * a component interaction test; it does not claim visual browser coverage. */
function expandableBreadcrumb(props:ComponentProps<typeof WorkspaceBreadcrumb>){
 const require=breadcrumbRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),source=breadcrumbRead(breadcrumbResolve('packages/workplace/src/components/workspace-breadcrumb.tsx'),'utf8'),tree=ts.createSourceFile('actual-breadcrumb.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),declarations:import('typescript').FunctionDeclaration[]=[];
 ts.forEachChild(tree,node=>{if(ts.isFunctionDeclaration(node)&&node.name?.text==='WorkspaceBreadcrumb')declarations.push(node);});expect(declarations).toHaveLength(1);const declaration=declarations[0];if(!declaration)throw Error('Actual breadcrumb declaration missing');
 const code=ts.transpileModule(declaration.getText(tree).replace(/^export\s+/,''),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,jsxFactory:'breadcrumbElement'}}).outputText;
 let open=false;const evaluate=new Function('ports',`const {breadcrumbElement,Button,Link,usePathname,useSearchParams,ChevronRight,MoreHorizontal,useEffect,useRef,useState,navigationTrail,workplaceNavigation,styles}=ports;\n${code}\nreturn WorkspaceBreadcrumb;`) as (ports:Record<string,unknown>)=>typeof WorkspaceBreadcrumb;
 const component=evaluate({breadcrumbElement:h,Button:BreadcrumbButton,Link:WorkspaceLink,usePathname:()=>new URL(state.path,'https://entry.invalid').pathname,useSearchParams:()=>new URL(state.path,'https://entry.invalid').searchParams,ChevronRight:BreadcrumbChevron,MoreHorizontal:BreadcrumbMore,useEffect:()=>{},useRef:()=>({current:null}),useState:()=>[open,(next:boolean|((value:boolean)=>boolean))=>{open=typeof next==='function'?next(open):next;}],navigationTrail:breadcrumbTrail,workplaceNavigation:breadcrumbNavigation,styles:new Proxy({}, {get:(_target,key)=>String(key)})});
 return {render:()=>component(props),open(){const trigger=find<{onClick:()=>void;'aria-label'?:string}>(component(props),BreadcrumbButton,value=>value['aria-label']==='Show breadcrumb path');expect(trigger).toBeDefined();if(!trigger)throw Error('Actual breadcrumb overflow trigger missing');trigger.props.onClick();}};
}
it('renders the authorized product ancestor as a full navigation anchor in both desktop and opened overflow breadcrumbs',async()=>{
 state.host='admin-dev-erpx.jesadakorn.com';state.path=breadcrumbDepartment+'/administration/technology/directory/objects';useBreadcrumbWorkspace(breadcrumbContext());const props=layoutBreadcrumb(await layout()),component=expandableBreadcrumb(props);
 expect(find(component.render(),'div',(value:{'aria-label'?:string})=>value['aria-label']==='Breadcrumb path')).toBeUndefined();component.open();const expanded=component.render(),menu=find<{children?:ReactNode;'aria-label'?:string}>(expanded,'div',value=>value['aria-label']==='Breadcrumb path');expect(menu).toBeDefined();
 const desktop=breadcrumbAnchors(breadcrumbMarkup(props)),overflow=breadcrumbAnchors(renderToStaticMarkup(h(ContextSwitchContext,{value:{busy:false,switchContext:async()=>false}},menu)));
 for(const base of [breadcrumbHolding,breadcrumbBranch]){const expected={href:base+'/administration',fullNavigation:true};expect(desktop).toContainEqual(expected);expect(overflow).toContainEqual(expected);}
 expect(overflow.some(anchor=>anchor.href===breadcrumbHolding+'/home'||anchor.href===breadcrumbBranch+'/home')).toBe(false);expect(fetch).not.toHaveBeenCalled();
});


import {WorkContextBoundary as InitialContextBoundary} from '@erpx/workplace/components/work-context-boundary';
it.each([true,false])('passes only the current public SSR identity to the actual layout boundary (source-only=%s)',async sourceOnly=>{
 const started=Date.now(),context=actor();reads.access.mockImplementation(async()=>({sourceRecord:sourceOnly,target:state.path,chooseRole:'/choose-work-role'}));
 if(!sourceOnly)useBreadcrumbWorkspace(context);
 const page=await layout(),boundary=find<ComponentProps<typeof InitialContextBoundary>>(page,InitialContextBoundary);expect(boundary).toBeDefined();const initial=boundary!.props.initialCurrent;expect(initial).toEqual({version:1,base:'/holdings/acme',userId:context.userId,companyId:context.companyId,assignmentId:context.assignmentId,revision:context.authority!.revision,checkedAt:expect.any(Number)});expect(initial!.checkedAt).toBeGreaterThanOrEqual(started);expect(initial!.checkedAt).toBeLessThanOrEqual(Date.now());expect(Object.keys(initial!).sort()).toEqual(['assignmentId','base','checkedAt','companyId','revision','userId','version']);expect(JSON.stringify(initial)).not.toContain('private-session-marker');expect(JSON.stringify(initial)).not.toContain('private-permission-marker');expect(fetch).not.toHaveBeenCalled();
});


import {AdminWorkspaceMenus as QuerySafeAdminMenus} from '@erpx/workplace/components/admin-workspace-menus';
it('keeps an ordinary Admin ancestor landing when its untouched query contains an ERPX Management return path',async()=>{
 state.host='admin-dev-erpx.jesadakorn.com';state.path=breadcrumbDepartment+'/administration/technology/directory/objects?returnTo=/erpx-management#keep';const context=breadcrumbContext();useBreadcrumbWorkspace(context);
 const page=await layout(),props=layoutBreadcrumb(page);expect(props.ancestors).toEqual([{label:'Actual Holding',href:breadcrumbHolding+'/administration',fullNavigation:true},{label:'North Branch',href:breadcrumbBranch+'/administration',fullNavigation:true},{label:'Design Department',href:breadcrumbDepartment+'/administration',fullNavigation:false}]);
 expect(reads.adminAccess).toHaveBeenCalledExactlyOnceWith(context.companyId,state.path);const menu=find<ComponentProps<typeof QuerySafeAdminMenus>>(page,QuerySafeAdminMenus);expect(menu).toBeDefined();expect(menu!.props.management).toBe(false);expect(breadcrumbAnchors(breadcrumbMarkup(props)).some(anchor=>anchor.href?.includes('/erpx-management'))).toBe(false);expect(fetch).not.toHaveBeenCalled();
});
