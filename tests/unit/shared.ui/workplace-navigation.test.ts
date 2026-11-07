import {products,productIds,sharedHostKeys} from '@erpx/contracts/product-registry';
import {groupAppsSidebar,groupHospitalitySidebar,sidebarSelection} from '@erpx/feature-work_projection/lib/sidebar-catalog';
import {SidebarSection,sidebarInitialGroup} from '@erpx/ui/components/sidebar-navigation';
import {InboxUpdates} from "@erpx/feature-collaboration/components/messaging/inbox-updates";
import {RecordLink} from "@erpx/feature-records/components/record-link";
import {RecordScopeProvider} from "@erpx/feature-records/public/client";
import {createElement as h,type ReactNode} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {productNavigationHref,productHrefString,frontProductLaunchTarget,isProductHandoffHref,type ProductNavigationScope} from '@erpx/feature-work_projection/lib/product-navigation';
import {AppsHomeLink,ProductLinkEnvironment,ProductWorkLinks,useProductNavigationScope} from '@erpx/workplace/components/product-link-context';
import WorkspaceLink from '@erpx/workplace/components/workspace-link';
import {useRouter as useDestinationRouter} from '@erpx/feature-shared/lib/destination-router';
const linkMocks=vi.hoisted(()=>({push:vi.fn(),replace:vi.fn(),dirty:vi.fn(()=>false),dispatch:vi.fn()}));
vi.mock('next/navigation',()=>({useRouter:()=>({push:linkMocks.push,replace:linkMocks.replace}),usePathname:()=>'/holdings/acme/services/assets/register',useSearchParams:()=>new URLSearchParams()}));
vi.mock("@erpx/feature-work_projection/lib/context-switch",()=>({hasUnsavedWork:linkMocks.dirty}));
vi.mock('next/link',async()=>{const {createElement}=await import('react');return {default:({href,children,prefetch,...props}:{href:string;children:ReactNode;prefetch?:boolean})=>createElement('a',{...props,href,'data-prefetch':String(prefetch)},children)};});
afterEach(()=>{vi.unstubAllGlobals();linkMocks.dirty.mockReturnValue(false);linkMocks.push.mockClear();linkMocks.replace.mockClear();linkMocks.dispatch.mockClear();});
import {describe,it,expect,vi,afterEach} from "vitest";
import {workplaceNavigation,navigationTrail,resolveNavigation,navigationCategory,navigationCollapsedValue,pmsSidebarHref} from "@erpx/feature-work_projection/lib/workplace-navigation";
import {navigationLauncher} from "@erpx/feature-work_projection/lib/navigation-catalog";
import {destinationWorkEntry, workEntries, navigationWorkEntry, navigationTopicHref} from "@erpx/feature-work_projection/lib/navigation-presentation";
import {destinations} from "@erpx/feature-work_projection/lib/destination-registry";
import {builtInPermissions} from "@erpx/feature-identity_access/lib/permissions";
import {workViews,filterWork,workTypes,type WorkItem} from "@erpx/feature-work_projection/lib/work-item";
import {pmsCrossLinks} from "@erpx/feature-pms/lib/pms-cross-links";
const base="/holdings/acme";
const owner=workplaceNavigation({workspaceBase:base,role:"holding_owner",canManageApprovals:true,adminSections:["people","organization","approvals","usage","ai","feedback"]});
describe("Main work entry presentation",()=>{
 it("retains Company, hotel and accounting scope while dropping old record state and cursors",()=>{
  const query=new URLSearchParams('companyId=original&property=hotel-one&chartId=chart-one&cursor=old&detailId=old&q=old&companyId=second');
  const bank=new URL(navigationTopicHref(base+'/finance/cash/bank',query),'https://test.invalid');
  expect(bank.searchParams.getAll('companyId')).toEqual(['original','second']);expect(bank.searchParams.get('property')).toBe('hotel-one');
  expect(bank.searchParams.get('chart')).toBe('chart-one');expect(bank.searchParams.has('chartId')).toBe(false);
  for(const key of ['cursor','detailId','q'])expect(bank.searchParams.has(key)).toBe(false);
  const invoices=new URL(navigationTopicHref(base+'/finance/documents/receivable?queue=open',query),'https://test.invalid');
  expect(invoices.searchParams.get('chartId')).toBe('chart-one');expect(invoices.searchParams.get('queue')).toBe('open');expect(invoices.searchParams.has('chart')).toBe(false);
 });
 it("keeps the five platform entries inside the authorized ERPX holding",()=>{
  const own=workplaceNavigation({workspaceBase:'/holdings/erpx',scopeKind:'holding',role:'holding_owner',manageErpxVisible:true});
  expect(own.categories.find(category=>category.id==='erpx-management')?.apps).toHaveLength(5);
  for(const workspaceBase of [base,'/holdings/erpx/branches/main'])expect(workplaceNavigation({workspaceBase,scopeKind:workspaceBase===base?'holding':'branch',role:'holding_owner',manageErpxVisible:true}).categories.some(category=>category.id==='erpx-management')).toBe(false);
 });
 it("covers every registered destination with 60 company entries and five separate platform entries",()=>{
  expect(workEntries.filter(item=>item.categoryId!=="erpx-management")).toHaveLength(60);
  expect(workEntries.filter(item=>item.categoryId==="erpx-management")).toHaveLength(5);
  for(const destination of destinations) expect(destinationWorkEntry(destination),destination.path).toBeTruthy();
  const model=workplaceNavigation({workspaceBase:base,role:"holding_owner",moduleIds:["talent","benefits","marketing","ecommerce","assets","maintenance","field_service","planning","manufacturing","learning","agent","quality","operations"],adminSections:["brand","organization","people","approvals","ai","usage","feedback"],canManageApprovals:true,handbookAccess:{editor:true,owner:true}});
  expect(model.categories.map(category=>category.apps.length)).toEqual([9,6,3,6,3,4,6,4,4,2,3,1,9]);
  const topics=model.categories.flatMap(category=>category.apps.flatMap(item=>item.submodules??[]));
  for(const page of model.search.filter(item=>item.workEntryId&&!['home','work','calendar','inbox'].includes(item.workEntryId)))expect(topics.some(topic=>topic.href.split('?')[0]===page.href.split('?')[0])||model.categories.some(category=>category.apps.some(root=>root.href===page.href)),page.href).toBe(true);
  for(const root of model.categories.flatMap(category=>category.apps)){const paths=root.submodules?.map(item=>new URL(item.href,'https://test.invalid').pathname)??[];expect(new Set(paths).size,root.id).toBe(paths.length);}
 });
 it("selects the permitted payslip landing without exposing payroll administration",()=>{
  const model=workplaceNavigation({workspaceBase:base,permissions:["people.read"],availableAppIds:["people"]});
  const payroll=model.categories.flatMap(category=>category.apps).find(item=>item.id==='people-payroll');
  expect(payroll?.href).toBe(base+'/people-organization/people/payslips');
  expect(payroll?.submodules?.map(item=>item.href)).toEqual([payroll!.href]);
  expect(navigationWorkEntry(model,base,payroll!.href)?.id).toBe('people-payroll');
 });
 it.each(['holding','branch','department'] as const)("keeps explicit ownership and authorized landings in %s scope",scopeKind=>{
  const workspaceBase=base+(scopeKind==='holding'?'':scopeKind==='branch'?'/branches/main':'/branches/main/departments/team');
  const model=workplaceNavigation({workspaceBase,scopeKind,role:'holding_owner'});
  for(const [path,id] of [['people-organization/people/organization','people-directory'],['people-organization/people/payslips','people-payroll'],['inventory-quality/transit','operations-transfers'],['services/assets/maintenance','assets'],['services/legal/contract','legal'],['administration/pilot-features','technology-business-readiness']] as const){
   const selected=navigationWorkEntry(model,workspaceBase,workspaceBase+'/'+path);
   if(model.search.some(item=>item.href===workspaceBase+'/'+path))expect(selected?.id).toBe(id);
   expect(selected?.id).not.toBe('people-roster');
  }
  for(const root of model.categories.flatMap(category=>category.apps))expect(model.search.some(item=>item.href===root.href),root.id).toBe(true);
 });
 it("hides empty categories and continuations that require a named record proof",()=>{
  const model=workplaceNavigation({workspaceBase:base,permissions:[],moduleIds:[],availableAppIds:[]});
  expect(model.categories.every(category=>category.apps.length>0)).toBe(true);
  for(const id of ['finance','people-organization','administration','erpx-management'])expect(model.categories.some(category=>category.id===id)).toBe(false);
  for(const suffix of ['services/assets/accounting','services/field-service/settings/parts','knowledge/learning/settings/learners'])expect(owner.search.some(item=>item.href===base+'/'+suffix)).toBe(false);
 });
});
describe("Side menu destination coverage",()=>{
 it.each(['apps','finance','people','pos','pms'] as const)("shows the complete authorized Office menu on %s", productId => {
  const input={workspaceBase:base,role:"holding_owner",moduleIds:["finance","people_payroll","pos","pms","crm","sales","operations","support"],canManageApprovals:true,adminSections:["people","organization","approvals","feedback"] as ("people"|"organization"|"approvals"|"feedback")[]};
  const complete=workplaceNavigation(input), actual=workplaceNavigation({...input,productId});
  expect(actual).toEqual(complete);
  expect(actual.primary.some(item=>item.id==='home')).toBe(true);
  expect(actual.search.some(item=>item.href.includes('/finance/'))).toBe(true);
  expect(actual.search.some(item=>item.href.includes('/people-organization/'))).toBe(true);
  expect(actual.categories.every(category=>category.apps.length || category.secondaryItems?.length)).toBe(true);
 });
 it('preserves permission, module and readiness filtering on every Office host',()=>{
  const input={workspaceBase:base,role:'employee',moduleIds:['operations'],permissions:['records.read'],availableAppIds:['operations']};
  for(const productId of ['apps','finance','people'] as const){
    const actual=workplaceNavigation({...input,productId});
    expect(actual).toEqual(workplaceNavigation(input));
    expect(actual.apps.some(app=>app.id==='finance'||app.id==='people')).toBe(false);
    expect(actual.categories.every(category=>category.apps.length || category.secondaryItems?.length)).toBe(true);
  }
 });
 it("keeps ERPX Management in its authorized holding placement",()=>{
  const model=workplaceNavigation({workspaceBase:"/holdings/erpx",scopeKind:"holding",role:"holding_owner",manageErpxVisible:true});
  expect(model.apps.find(app=>app.id==="erpx-management")?.items.map(i=>i.href)).toEqual(["/holdings/erpx/erpx-management","/holdings/erpx/erpx-management/billing","/holdings/erpx/erpx-management/usage","/holdings/erpx/erpx-management/iam","/holdings/erpx/erpx-management/executive"]);
  for(const ctx of [{workspaceBase:base,scopeKind:"holding" as const},{workspaceBase:"/holdings/erpx/branches/main",scopeKind:"branch" as const}])expect(workplaceNavigation({...ctx,role:"holding_owner",manageErpxVisible:true}).apps.some(a=>a.id==="erpx-management")).toBe(false);
 });
 it("shows only approved pilot apps in production navigation",()=>{
  const pilot=workplaceNavigation({workspaceBase:base,role:"holding_owner",availableAppIds:["people","inventory","pos","pms","finance"]});
  expect(pilot.search.some(i=>i.id==="crm-pipeline"||i.id==="finance-engines")).toBe(false);
  expect(pilot.search.find(i=>i.id==="people-payroll")?.href).toBe(base+"/people-organization/people/payroll/dashboard");
  expect(pilot.search.some(i=>i.id==="future-features")).toBe(true);
 });
 it("makes destinations and retained shortcut IDs unambiguous",()=>{
  for(const role of ["employee","holding_owner","hr_admin","it_admin"]){
   const model=workplaceNavigation({workspaceBase:base,role,adminSections:["feedback"]});
   expect(new Set(model.search.map(i=>i.id)).size).toBe(model.search.length);
   const results=navigationLauncher(model," ").categories.flatMap(c=>c.apps);
   expect(new Set(results.map(i=>i.href)).size).toBe(results.length);
   for(const item of model.search){expect(item.href.startsWith(base+"/")).toBe(true);expect(new URL(item.href,"https://example.test").searchParams.has("tab")).toBe(false);}
   const found=navigationLauncher(model,"finance").results;expect(new Set(found.map(r=>r.item.href)).size).toBe(found.length);
  }
 });
 it("exposes Operations through its scoped sidebar and launcher using existing manager permissions",()=>{
  const permissions=builtInPermissions("operations_manager");expect(permissions).toContain("records.read");expect(permissions).toContain("module.operations.read");
  for(const [workspaceBase,scopeKind] of [[base,"holding"],["/holdings/erpx","holding"],[base+"/branches/bangkok","branch"]] as const){
   const model=workplaceNavigation({workspaceBase,scopeKind,role:"operations_manager",permissions,moduleIds:["operations"],availableAppIds:["operations"]});
   expect(model.categories.find(c=>c.id==="projects-routines")?.apps.map(i=>[i.id,i.label,i.href,i.ownerModuleId])).toEqual([["operations","Exception tracking",workspaceBase+"/projects-routines/operations","operations"]]);
   expect(navigationLauncher(model,"Operations").results.map(r=>[r.item.label,r.item.href,r.categoryId])).toEqual([["Operations",workspaceBase+"/projects-routines/operations","projects-routines"]]);
   expect(model.apps.some(a=>a.id==="finance")).toBe(false);
  }
 });
 it("keeps Operations hidden without its module read permission or rollout and does not expose Projects",()=>{
  const allowed={workspaceBase:base,role:"operations_manager",permissions:["records.read"],moduleIds:["operations"],availableAppIds:["operations"]};
  for(const context of [{...allowed,moduleIds:[]},{...allowed,moduleIds:["inventory"]},{...allowed,permissions:[]},{...allowed,availableAppIds:[]},{...allowed,availableAppIds:["projects"]},{...allowed,beta:true}]){
   const model=workplaceNavigation(context);expect(model.apps.some(a=>a.id==="operations")).toBe(false);expect(model.search.some(i=>i.href===base+"/projects-routines/operations")).toBe(false);
  }
  const model=workplaceNavigation(allowed);expect(model.apps.some(a=>a.id==="operations")).toBe(true);expect(model.apps.some(a=>a.id==="projects")).toBe(false);expect(model.search.some(i=>i.href===base+"/projects-routines/projects")).toBe(false);
 });
 it.each([
  ["administration/technology/directory/connections",["Administration","Technology","Directory","Connections"],"technology"],
  ["people-organization/people/directory/current",["People & organization","People","Employees","Current"],"people"],
  ["people-organization/people/hr-settings/leave",["People & organization","People","HR settings","Leave & overtime"],"people"],
  ["customers-sales/sales/documents/quote",["Customers & sales","Sales","Documents","Quotations"],"sales"],
  ["inventory-quality/products",["Inventory & quality","Products"],"inventory"],
  ["finance/accounting/engines",["Finance","Accounting","Calculations & schedules"],"finance"]
 ] as const)("resolves %s with every clickable hierarchy level",(path,labels,groupId)=>{
  const crumbs=navigationTrail(owner,base,base+"/"+path);expect(crumbs.map(c=>c.label)).toEqual(labels);expect(crumbs.every(c=>c.href?.startsWith(base+"/"))).toBe(true);expect(resolveNavigation(owner,base,base+"/"+path).groupId).toBe(groupId);
 });
});
describe("Workplace discovery",()=>{
 it("keeps role and scope restrictions in discovery",()=>{
  const inventory=workplaceNavigation({workspaceBase:base,permissions:["inventory.read"]});expect(inventory.apps.some(a=>["sales","procurement"].includes(a.id))).toBe(false);expect(inventory.search.some(i=>i.id==="people-settings")).toBe(false);
  for(const scopeKind of ["branch","department"] as const){const nav=workplaceNavigation({workspaceBase:base+"/branches/main"+(scopeKind==="department"?"/departments/front":""),scopeKind,role:"holding_owner"});expect(nav.search.some(i=>i.href.endsWith("/people/payroll/dashboard")||i.id==="technology-business-readiness")).toBe(false);if(scopeKind==="department")expect(nav.apps.some(a=>a.id==="pos"||a.id==="pms")).toBe(false);}
  const housekeeper=workplaceNavigation({workspaceBase:base,role:"pms_housekeeping"});expect(housekeeper.search.some(i=>["pms-cashier","pms-reports","pms-night-audit"].includes(i.id))).toBe(false);
 });
 it("keeps staff search inside authorized destinations",()=>{const nav=workplaceNavigation({workspaceBase:base,role:"employee",adminSections:["feedback"]});expect(nav.primary.map(i=>i.id)).toEqual(["home","work","calendar","inbox"]);expect(nav.search.some(i=>!i.id.startsWith("start-")&&i.href.includes("/finance"))).toBe(false);expect(nav.search.some(i=>i.href.includes("/modules/"))).toBe(false);});
 it("shows every My work view while honoring Team and Routing availability",()=>{
  for(const team of [false,true])for(const routing of [false,true]){const model=workplaceNavigation({workspaceBase:base,role:"holding_owner",workViewAvailability:{team,routing}});const views=model.primary.find(i=>i.id==="work")!.submodules!;
   expect(views.map(i=>i.href)).toEqual([...workViews.filter(([id])=>id!=="team"&&id!=="routing"||id==="team"&&team||id==="routing"&&routing).map(([id])=>base+"/work/"+id),base+"/work/reports"]);
  }
 });
 it("keeps daily submenu URLs explicit",()=>{expect(owner.primary.find(i=>i.id==="calendar")?.submodules?.map(i=>i.href)).toEqual([base+"/calendar/mine",base+"/calendar/team",base+"/calendar/coverage"]);expect(owner.primary.find(i=>i.id==="inbox")?.submodules?.map(i=>i.href)).toEqual([base+"/inbox/messages",base+"/inbox/teams",base+"/inbox/notifications",base+"/administration/notifications"]);});
 it("uses the server approval flag independently from readable sections",()=>{
  const readable=workplaceNavigation({workspaceBase:base,role:"employee",adminSections:["approvals"],canManageApprovals:false});expect(readable.search.some(i=>i.id==="administration-approvals")).toBe(false);
 });
 it("keeps Executive hidden without eligibility or a named share, and resolves report breadcrumbs",()=>{
  expect(workplaceNavigation({workspaceBase:base,role:"holding_owner",executiveVisible:false}).apps.some(a=>a.id==="executive")).toBe(false);const shared=workplaceNavigation({workspaceBase:base,role:"employee",executiveVisible:true});expect(shared.apps.find(a=>a.id==="executive")?.items).toHaveLength(1);expect(resolveNavigation(shared,base,base+"/insights/executive/reports/private-id/overview").groupId).toBe("executive");
 });
 it("carries only selected property between PMS sidebar pages",()=>{const reports=owner.apps.find(a=>a.id==="pms")!.items.find(i=>i.id==="pms-reports")!;expect(pmsSidebarHref(reports,base+"/pms/guest-accounts",new URLSearchParams("property=hotel-1&guestAccount=other"))).toBe(base+"/pms/reports?property=hotel-1");});
 it("keeps cross-module access and rollout independent",()=>{const actor={role:"holding_owner",assignmentScope:"holding",permissions:["pms.read","commerce.read","finance.read","pos.read"]};expect(pmsCrossLinks(actor)).toEqual({inventory:true,finance:true,pos:true});expect(pmsCrossLinks(actor,new Set(["pms","finance"]))).toEqual({inventory:false,finance:true,pos:false});});
 it("migrates retained expansion choices",()=>{expect(JSON.parse(navigationCollapsedValue(null,'["finance","resources","administration"]',["people","finance","technology"]))).toEqual(["finance","technology"]);});
 it("keeps Finance and PMS in their discovery categories",()=>{expect(navigationCategory(owner,resolveNavigation(owner,base,base+"/finance/cash/bank"))?.id).toBe("finance");expect(navigationCategory(owner,resolveNavigation(owner,base,base+"/pms/reports"))?.id).toBe("pms");});
});
it("filters mixed work without treating another queue as actionable",()=>{const rows:WorkItem[]=[{id:"same",type:"leave",title:"Leave",detail:"Morning handover",href:"/leave",status:"pending_approval",updatedAt:1,queue:"waiting"},{id:"same",type:"staff_task",title:"Handover",detail:"Review delivery",href:"/task",status:"completion_approval",updatedAt:2,queue:"todo"}];expect(filterWork(rows,"todo","all","handover").map(i=>i.type)).toEqual(["staff_task"]);expect(filterWork(rows,"all","leave","")).toHaveLength(1);expect(workTypes.map(([id])=>id)).not.toContain("record");});

describe('Product owner link transport',()=>{
 const scope:ProductNavigationScope={audience:'assets',companyId:'company-two',companySlug:'acme-co',workspaceBase:base};
 const unwrap=(href:string)=>{const sso=new URL(href);expect(sso.pathname).toBe('/sso/start');const launch=new URL(sso.searchParams.get('next')!,'https://launch.invalid');expect(launch.pathname).toBe('/launch');return {sso,launch,next:launch.searchParams.get('next')!};};
 it('preserves the selected Company, query, record and fragment through an Office owner handoff',()=>{
  const result=productNavigationHref(base+'/finance/invoices/invoice-1?status=paid&filter=a%2Fb#workflow-owners',scope);expect(result.handoff).toBe(true);const target=unwrap(result.href);expect(target.sso.origin).toBe('https://finance-dev-erpx.jesadakorn.com');expect(target.launch.searchParams.get('companyId')).toBe('company-two');expect(target.next).toBe(base+'/finance/documents/invoices/invoice-1/overview?status=paid&filter=a%2Fb#workflow-owners');
 });
 it('moves original Company Executive deep links to Work with their Company, node, report, repeated filters and canvas fragment',()=>{
  const scoped={...scope,workspaceBase:base+'/branches/main/departments/team'},path=scoped.workspaceBase+'/insights/executive/reports/original?tag=one&tag=two#canvas';
  const result=productNavigationHref('https://console-dev-erpx.jesadakorn.com'+path,scoped),target=unwrap(result.href);expect(result.handoff).toBe(true);expect(target.sso.origin).toBe('https://apps-dev-erpx.jesadakorn.com');expect(target.launch.searchParams.get('companyId')).toBe('company-two');expect(target.next).toBe(path);
  expect(productNavigationHref(path,{...scoped,audience:'apps'})).toEqual({href:path,handoff:false});
 });
 it('maps all Office product owners through the shared registry and leaves own-product links local',()=>{
  for(const [destination,owner] of [['home','apps'],['people-organization/people/directory/current','people'],['customers-sales/sales/pipeline','sales'],['purchasing/orders','supply'],['projects-routines/projects','projects'],['people-organization/talent','recruit'],['knowledge/learning','learn'],['services/desk','support'],['services/field-service','service'],['services/legal/mine','legal'],['production/manufacturing','manufacturing']] as const){const result=productNavigationHref(base+'/'+destination,scope);expect(result.handoff).toBe(true);expect(unwrap(result.href).sso.hostname).toBe(owner+'-dev-erpx.jesadakorn.com');}
  expect(productNavigationHref(base+'/services/assets/register?q=current',scope)).toEqual({href:base+'/services/assets/register?q=current',handoff:false});
  expect(productNavigationHref('https://console-dev-erpx.jesadakorn.com'+base+'/services/assets/register',scope)).toEqual({href:base+'/services/assets/register',handoff:false});
 });
 it('uses actual native Front routes for receipts, Hotel scope and location roots',()=>{
  const pos=unwrap(productNavigationHref(base+'/pos?tab=history&sale=sale-1',scope).href);expect(pos.sso.origin).toBe('https://pos-dev-erpx.jesadakorn.com');expect(pos.next).toBe('/acme/acme-co/holding/history/receipts/sale-1/overview');
  const branch={...scope,workspaceBase:base+'/branches/b1/departments/d1'},pms=unwrap(productNavigationHref(branch.workspaceBase+'/pms/guest-accounts?property=h1&guestAccount=g1#charges',branch).href);expect(pms.next).toBe('/acme/acme-co/branches/b1/departments/d1/guest-accounts?property=h1&guestAccount=g1#charges');
  expect(unwrap(productNavigationHref('/front/acme/acme-co/pms/holding/',scope).href).next).toBe('/acme/acme-co/holding/');
 });
 it('converts original Front shared-work and record links to normal selected-Company Office entry',()=>{
  const front={...scope,audience:'pms' as const};const result=productNavigationHref('https://console-dev-erpx.jesadakorn.com'+base+'/finance/journals/owned?frontReturn=%2Facme%2Facme-co%2Fholding%2Fguest-accounts#workflow-owners',front);expect(result.handoff).toBe(true);const target=unwrap(result.href);expect(target.launch.searchParams.get('companyId')).toBe('company-two');expect(target.next).toContain('/finance/accounting/journals/owned/overview?frontReturn=');expect(target.next).toContain('#workflow-owners');
  expect(unwrap(productNavigationHref('https://dev-pms.jesadakorn.com/acme/acme-co/holding/guest-accounts?property=h1',scope).href).next).toBe('/acme/acme-co/holding/guest-accounts?property=h1');
  expect(productNavigationHref('/front/acme/acme-co/pms/holding/rooms?property=h1',front)).toEqual({href:'/acme/acme-co/holding/rooms?property=h1',handoff:false});
 });
 it('keeps explicit operations, external/staging hosts and other Company scopes outside automatic handoffs',()=>{
  for(const href of ['/api/v1/holdings/acme/finance/invoices','/api/public/booking/callback','/sso/callback?code=secret','https://evil.test/holdings/acme/finance','https://erpx.jesadakorn.com/holdings/acme/finance','//evil.test/holdings/acme/finance'])expect(productNavigationHref(href,scope)).toEqual({href,handoff:false});
  expect(productNavigationHref(base+'/finance/invoices',{...scope,audience:null}).handoff).toBe(false);expect(productNavigationHref('/holdings/other/finance/invoices',scope).handoff).toBe(false);expect(productNavigationHref(base+'/branches/another/finance/invoices',scope).handoff).toBe(false);
  expect(unwrap(productNavigationHref(base+'/pms/reports',scope).href).next).toBe(base+'/pms/reports');
  expect(productNavigationHref(base+'/finance/invoices?code=private&state=stale&token=secret&status=paid',scope).href).not.toContain('private');
 });
 it('keeps Front backoffice and specialized records on their product host while retaining filters, fragments and native daily paths',()=>{
  for(const app of ['pos','pms'] as const){
   const local={...scope,audience:app};
   expect(productNavigationHref(base+'/'+app,local)).toEqual({href:base+'/'+app,handoff:false});
   for(const path of app==='pos'?['settings','reports','approvals','records/pos_report/source-1/overview']:['settings','setup','import','minibar','guests','reports','records/pms_booking_refund/source-1/overview']){
    const value=base+'/'+app+'/'+path+'?property=h1&tag=one&tag=two#history';
    expect(productNavigationHref('https://console-dev-erpx.jesadakorn.com'+value,local)).toEqual({href:value,handoff:false});
    const target=unwrap(productNavigationHref(value,scope).href);expect(target.sso.hostname).toBe(app+'-dev-erpx.jesadakorn.com');expect(target.next).toBe(value);
   }
  }
  for(const value of ['reservations?queue=groups','guest-accounts?queue=approval','guest-accounts?queue=corrections','cashier?queue=approval'])expect(unwrap(productNavigationHref(base+'/pms/'+value,scope).href).next).toBe(value==='guest-accounts?queue=approval'?base+'/pms/approvals':base+'/pms/'+value);
  expect(productNavigationHref(base+'/pos?tab=history&sale=s1',{...scope,audience:'pos'})).toEqual({href:'/acme/acme-co/holding/history/receipts/s1/overview',handoff:false});
 });
 it('supports Next relative URL objects without guessing an external URL-object origin',()=>{
  expect(productHrefString({pathname:base+'/finance/invoices',query:{status:['paid','posted'],empty:null},hash:'workflow-owners'})).toBe(base+'/finance/invoices?status=paid&status=posted&empty=#workflow-owners');expect(productHrefString({hostname:'evil.test',pathname:base+'/finance'})).toBeNull();
 });
 it('validates native Front launch owners and location shapes without a provider or API escape',()=>{
  expect(frontProductLaunchTarget('pms','/acme/acme-co/branches/b1/departments/d1/guest-accounts/records/pms_entry/e1/overview?property=h1#charges')).toMatchObject({holdingSlug:'acme',companySlug:'acme-co',location:'branches/b1/departments/d1'});
  for(const href of ['//evil.test','/acme/acme-co/holding/../api/auth/sign-out','/acme/acme-co/holding/sale','/acme/acme-co/holding/guest-accounts/records/invoice/i1/overview','/acme/acme-co/holding/guest-accounts/records/pms_record/e1/history','/acme/acme-co/holding/guest-accounts%2Fapi'])expect(frontProductLaunchTarget('pms',href),href).toBeNull();
  expect(isProductHandoffHref('https://finance-dev-erpx.jesadakorn.com/sso/start?next=/')).toBe(true);expect(isProductHandoffHref('https://evil.test/sso/start')).toBe(false);expect(isProductHandoffHref('https://erpx.jesadakorn.com/sso/start')).toBe(false);
 });
 const tree=(child:ReactNode)=>h(ProductLinkEnvironment,{audience:'assets',children:h(ProductWorkLinks,{companyId:'company-two',companySlug:'acme-co',workspaceBase:base,children:child})});
 it('keeps every brand on the configured Apps entry across nested workspace scopes and domain changes',()=>{
  for(const audience of [...productIds,...sharedHostKeys])for(const appsHref of ['https://apps-dev-erpx.jesadakorn.com/sso/start?next=%2F','https://apps.example.test/sso/start?next=%2F','/']){
   const html=renderToStaticMarkup(h(ProductLinkEnvironment,{audience,appsHref,children:h(ProductWorkLinks,{companyId:'company-two',workspaceBase:base,children:h(AppsHomeLink,{className:'brand'},'ERPX')})}));
   expect(html).toContain(`href="${appsHref}"`);expect(html).toContain('aria-label="ERPX Apps"');expect(html).toContain('data-product-switch="true"');expect(html).toContain('data-prefetch="false"');expect(html).not.toContain('company-two');
  }
 });
 it('renders actual cross-product anchors before hydration with a dirty-guard marker and no SSO prefetch',()=>{
  const html=renderToStaticMarkup(tree(h(WorkspaceLink,{href:base+'/finance/invoices/invoice-1',prefetch:true},'Owned invoice')));expect(html).toContain('https://finance-dev-erpx.jesadakorn.com/sso/start');expect(html).toContain('data-product-switch="true"');expect(html).toContain('data-prefetch="false"');expect(html).toContain('company-two');
  const explicit=renderToStaticMarkup(tree(h(WorkspaceLink,{href:'https://apps-dev-erpx.jesadakorn.com/sso/start?next=/',...{'data-product-switch':true}},'Apps')));expect(explicit).toContain('data-product-switch="true"');
 });
 it('renders cross-product RecordLinks without collapsing repeated filters or the selected fragment into a local drawer',()=>{
  const html=renderToStaticMarkup(tree(h(RecordScopeProvider,{base,children:h(RecordLink,{href:base+'/finance/invoices/i1?tag=one&tag=two#workflow-owners'},'Invoice')})));
  const href=/href="([^"]+)"/.exec(html)![1].replaceAll('&amp;','&'),target=unwrap(href),record=new URL(target.next,'https://record.invalid');expect(record.pathname).toBe(base+'/finance/documents/invoices/i1/overview');expect(record.searchParams.getAll('tag')).toEqual(['one','two']);expect(record.hash).toBe('#workflow-owners');expect(record.searchParams.has('returnTo')).toBe(false);
 });
 it('routes imperative handoffs through the existing dirty guard while keeping normal same-product navigation local',()=>{
  let router:ReturnType<typeof useDestinationRouter>|undefined;function Capture(){router=useDestinationRouter();return null;}
  vi.stubGlobal('window',{location:{href:'https://assets-dev-erpx.jesadakorn.com'+base+'/services/assets/register',origin:'https://assets-dev-erpx.jesadakorn.com'},dispatchEvent:linkMocks.dispatch});renderToStaticMarkup(tree(h(Capture)));
  router!.push(base+'/finance/invoices/invoice-1');expect(linkMocks.push.mock.calls[0][0]).toContain('https://finance-dev-erpx.jesadakorn.com/sso/start');linkMocks.push.mockClear();
  linkMocks.dirty.mockReturnValue(true);router!.push(base+'/finance/invoices/invoice-1');expect(linkMocks.push).not.toHaveBeenCalled();expect(linkMocks.dispatch).toHaveBeenCalledTimes(1);const event=linkMocks.dispatch.mock.calls[0][0];expect(event.type).toBe('erpx:product-navigate');expect(unwrap(event.detail.href).launch.searchParams.get('companyId')).toBe('company-two');
  router!.replace(base+'/services/assets/register?q=retained');expect(linkMocks.replace).toHaveBeenCalledWith(base+'/services/assets/register?q=retained',undefined);
 });
 it('preserves exact return query state when a drawer closes on the same owned route',()=>{
  let router:ReturnType<typeof useDestinationRouter>|undefined;function Capture(){router=useDestinationRouter();return null;}
  const finance=h(ProductLinkEnvironment,{audience:'finance',children:h(ProductWorkLinks,{companyId:'company-two',companySlug:'acme-co',workspaceBase:base,children:h(Capture)})}),origin='https://finance-dev-erpx.jesadakorn.com',list=base+'/finance/documents/sales/invoices?tab=invoices&q=issued&tag=one&tag=two#invoices';
  vi.stubGlobal('window',{location:{href:origin+base+'/finance/documents/invoices/invoice-1/overview',origin},dispatchEvent:linkMocks.dispatch});renderToStaticMarkup(finance);router!.pushPreservingQuery(list,{scroll:false});
  expect(linkMocks.push).toHaveBeenCalledExactlyOnceWith(list,{scroll:false});
  linkMocks.push.mockClear();router!.push(list,{scroll:false});expect(linkMocks.push).toHaveBeenCalledExactlyOnceWith(base+'/finance/documents/sales/invoices?q=issued&tag=one&tag=two#invoices',{scroll:false});
 });
});


it('renders authorized notification items in initial server content without message or notice-composer UI for foundation-only work',()=>{
 const item={id:'delivery-1',eventId:'event-1',category:'notice' as const,message:'Reviewed source notice',href:null,createdAt:1700000000000,readAt:null,archivedAt:null,sourceType:'notice',sourceId:'notice-1'},initial={items:[item],nextCursor:'next-public-cursor',unread:1};
 const html=renderToStaticMarkup(h(InboxUpdates,{base,canBroadcast:false,notificationsOnly:true,initial}));expect(html).toContain('Reviewed source notice');expect(html).toContain('Mark read');expect(html).toContain('Load more updates');expect(html).not.toContain('Send a notice');expect(html).not.toContain('Named recipients');expect(html).not.toContain('No matching updates.');
 const full=renderToStaticMarkup(h(InboxUpdates,{base,canBroadcast:false,initial}));expect(full).toContain('Reviewed source notice');expect(full).toContain('Send a notice');
});

describe('Shared Company Admin owner navigation',()=>{
 const scope:ProductNavigationScope={audience:'assets',companyId:'company-two',companySlug:'acme-co',workspaceBase:base};
 function unwrapAdmin(href:string){const sso=new URL(href),launch=new URL(sso.searchParams.get('next')!,'https://admin.invalid');expect(sso.origin).toBe('https://admin-dev-erpx.jesadakorn.com');expect(sso.pathname).toBe('/sso/start');expect(launch.pathname).toBe('/admin/launch');expect(launch.searchParams.get('companyId')).toBe('company-two');return launch.searchParams.get('next')!;}
 it('maps canonical and legacy Administration through its registered shared host preserving repeated filters and fragment',()=>{
  for(const value of ['/administration','/administration/technology/settings/roles','/administration/brand/identity','/settings/notifications']){const result=productNavigationHref(base+value+'?tag=one&tag=two#review',scope);expect(result.handoff).toBe(true);const target=new URL(unwrapAdmin(result.href),'https://target.invalid');expect(target.pathname).toBe(value==='/settings/notifications'?base+'/administration/notifications':base+value);expect(target.searchParams.getAll('tag')).toEqual(['one','two']);expect(target.hash).toBe('#review');}
  const branch={...scope,audience:'pms' as const,workspaceBase:base+'/branches/b1/departments/d1'},value=branch.workspaceBase+'/administration/technology/organization/department/d1/policies?q=current#access';expect(unwrapAdmin(productNavigationHref(value,branch).href)).toBe(value);expect(productNavigationHref(value,{...branch,audience:'admin'})).toEqual({href:value,handoff:false});
 });
 it('does not reinterpret foreign Company hints, location changes, APIs, provider callbacks or unregistered admin pages',()=>{
  for(const key of ['company','companyId','company_id']){const value=base+'/administration/technology/settings/roles?'+key+'=other';expect(productNavigationHref(value,scope).handoff).toBe(false);}
  for(const value of ['/api/v1/holdings/acme/iam','/sso/callback?code=private',base+'/branches/other/administration',base+'/administration/not-a-registered-area','https://evil.test'+base+'/administration','https://erpx.jesadakorn.com'+base+'/administration'])expect(productNavigationHref(value,scope).handoff).toBe(false);
  const result=productNavigationHref(base+'/administration/technology/settings/roles?code=private&token=secret&tag=one&tag=two#review',scope),target=new URL(unwrapAdmin(result.href),'https://target.invalid');expect(target.searchParams.has('code')).toBe(false);expect(target.searchParams.has('token')).toBe(false);expect(target.searchParams.getAll('tag')).toEqual(['one','two']);
 });
 it('maps ERPX Management only from its exact current ERPX Holding scope without inferring provider authority',()=>{
  const erpx={...scope,workspaceBase:'/holdings/erpx'},value='/holdings/erpx/erpx-management/iam/reviews?tab=current#review';expect(unwrapAdmin(productNavigationHref(value,erpx).href)).toBe(value);expect(productNavigationHref(base+'/erpx-management/iam/reviews',scope).handoff).toBe(false);expect(productNavigationHref(value,scope).handoff).toBe(false);expect(productNavigationHref(value,{...erpx,workspaceBase:'/holdings/erpx/branches/a'}).handoff).toBe(false);
 });
 it('sends imperative Admin handoffs through the existing dirty guard without a local source-host navigation',()=>{
  let router:ReturnType<typeof useDestinationRouter>|undefined;function Capture(){router=useDestinationRouter();return null;}
  const tree=h(ProductLinkEnvironment,{audience:'assets',children:h(ProductWorkLinks,{companyId:'company-two',companySlug:'acme-co',workspaceBase:base,children:h(Capture)})});linkMocks.push.mockClear();linkMocks.dispatch.mockClear();linkMocks.dirty.mockReturnValue(true);vi.stubGlobal('window',{location:{href:'https://assets-dev-erpx.jesadakorn.com'+base+'/services/assets/register',origin:'https://assets-dev-erpx.jesadakorn.com'},dispatchEvent:linkMocks.dispatch});try{renderToStaticMarkup(tree);router!.push(base+'/administration/technology/settings/roles?tag=one&tag=two#review');expect(linkMocks.push).not.toHaveBeenCalled();expect(linkMocks.dispatch).toHaveBeenCalledTimes(1);const event=linkMocks.dispatch.mock.calls[0][0];expect(event.type).toBe('erpx:product-navigate');expect(unwrapAdmin(event.detail.href)).toBe(base+'/administration/technology/settings/roles?tag=one&tag=two#review');}finally{linkMocks.dirty.mockReturnValue(false);vi.unstubAllGlobals();}
 });
 it('renders WorkspaceLink, action-link account notification and RecordLink Admin handoffs before hydration with the existing dirty marker',async()=>{
  const {ActionLink}=await import('@erpx/ui/components/foundation'),tree=(child:ReactNode)=>h(ProductLinkEnvironment,{audience:'assets',children:h(ProductWorkLinks,{companyId:'company-two',companySlug:'acme-co',workspaceBase:base,children:child})});
  for(const child of [h(WorkspaceLink,{href:base+'/administration/technology/settings/roles?tag=one&tag=two#roles',prefetch:true},'Roles'),h(ActionLink,{appearance:'standard',href:base+'/settings/notifications'},'Notification settings'),h(RecordLink,{href:base+'/administration/technology/organization/department/d1/policies#owner'},'Department')]){const html=renderToStaticMarkup(tree(child));expect(html).toContain('data-product-switch="true"');expect(html).toContain('data-prefetch="false"');const href=/href="([^"]+)"/.exec(html)![1].replaceAll('&amp;','&');expect(unwrapAdmin(href)).toContain('/administration/');expect(new URL(unwrapAdmin(href),'https://target.invalid').searchParams.has('returnTo')).toBe(false);}
 });
});

describe('Account self link transport without a Company role',()=>{
 const scope:ProductNavigationScope={audience:'admin'};
 const unwrap=(href:string)=>{const bridge=new URL(href);expect(bridge.origin).toBe('https://account-dev-erpx.jesadakorn.com');expect(bridge.pathname).toBe('/account/launch');expect([...bridge.searchParams.keys()]).toEqual(['next']);return bridge.searchParams.get('next')!;};
 it('maps the three explicit self pages from Office, Front and Platform-only shared hosts and retains repeated query and fragment',()=>{
  for(const audience of ['apps','pos','pms','admin'] as const)for(const path of ['/my-access','/security','/my-profile']){const value=path+'?tag=one&tag=two#details',result=productNavigationHref(value,{audience});expect(result.handoff).toBe(true);expect(unwrap(result.href)).toBe(value);expect(isProductHandoffHref(result.href)).toBe(true);}
  expect(unwrap(productNavigationHref('https://console-dev-erpx.jesadakorn.com/my-access?tag=one#roles',scope).href)).toBe('/my-access?tag=one#roles');
 });
 it('uses the already selected central Account locally and never forwards callback credentials',()=>{
  for(const path of ['/my-access','/security','/my-profile']){const value=path+'?code=secret&state=secret&token=secret&session_token=secret&_rsc=private&tag=one&tag=two#details',clean=path+'?tag=one&tag=two#details';expect(productNavigationHref(value,{audience:'account'})).toEqual({href:clean,handoff:false});expect(unwrap(productNavigationHref(value,scope).href)).toBe(clean);}
 });
 it('does not reinterpret member profiles, malformed aliases, API requests, callback paths, external or staging origins',()=>{
  for(const value of ['/holdings/acme/people-organization/profiles/user/overview','/my-profile/user','/my-profile/','/x/../my-access','/%6dy-access','/api/v1/account/profile','/sso/callback?code=private','//evil.test/my-access','https://evil.test/my-access','https://erpx.jesadakorn.com/my-access','https://user:secret@account-dev-erpx.jesadakorn.com/my-access','/my-access?bad=%zz'])expect(productNavigationHref(value,scope).handoff,value).toBe(false);
  expect(productNavigationHref('/my-access',{audience:null})).toEqual({href:'/my-access',handoff:false});
  for(const value of ['https://evil.test/account/launch?next=/my-access','https://account-dev-erpx.jesadakorn.com/account/launch?next=/holdings/acme/administration','https://account-dev-erpx.jesadakorn.com/account/launch?next=/security&next=/my-access','https://account-dev-erpx.jesadakorn.com/account/launch?next=/security&token=private'])expect(isProductHandoffHref(value)).toBe(false);
 });
 it('uses the actual imperative dirty guard for Platform-only Account handoffs without a local navigation',()=>{
  let router:ReturnType<typeof useDestinationRouter>|undefined;function Capture(){router=useDestinationRouter();return null;}
  linkMocks.dirty.mockReturnValue(true);vi.stubGlobal('window',{location:{href:'https://admin-dev-erpx.jesadakorn.com/provider',origin:'https://admin-dev-erpx.jesadakorn.com'},dispatchEvent:linkMocks.dispatch});renderToStaticMarkup(h(ProductLinkEnvironment,{audience:'admin',children:h(Capture)}));router!.push('/security?tag=one&tag=two#sessions');expect(linkMocks.push).not.toHaveBeenCalled();expect(linkMocks.dispatch).toHaveBeenCalledTimes(1);const event=linkMocks.dispatch.mock.calls[0][0];expect(event.type).toBe('erpx:product-navigate');expect(unwrap(event.detail.href)).toBe('/security?tag=one&tag=two#sessions');
 });
 it('renders actual Account WorkspaceLink and action links with the existing switch marker and no SSO prefetch before hydration',async()=>{
  const {ActionLink}=await import('@erpx/ui/components/foundation');
  for(const child of [h(WorkspaceLink,{href:'/my-access?tag=one&tag=two#roles',prefetch:true},'My access'),h(ActionLink,{appearance:'standard',href:'/my-profile#profile'},'My profile')]){const html=renderToStaticMarkup(h(ProductLinkEnvironment,{audience:'admin',children:child}));expect(html).toContain('data-product-switch="true"');expect(html).toContain('data-prefetch="false"');const href=/href="([^"]+)"/.exec(html)![1].replaceAll('&amp;','&');expect(unwrap(href)).toMatch(/^\/(?:my-access|my-profile)/);expect(href).not.toContain('companyId');}
 });
});

describe('actual owner cross-product links',()=>{
 const tree=(child:ReactNode)=>h(ProductLinkEnvironment,{audience:'pms',children:h(ProductWorkLinks,{companyId:'company-two',companySlug:'acme-co',workspaceBase:base,children:child})});
 it('renders the actual collaboration inbox handoff from Hotel while keeping CSV on its original API',async()=>{
  const {CollaborationWorkConsole}=await import('@erpx/feature-collaboration/components/collaboration-work-console');
  const data:Parameters<typeof CollaborationWorkConsole>[0]['data']={relations:[],cards:[{id:'card',version:1,status:'open',owner_id:'owner',created_by:'author',updated_at:1,due_at:1,owner_eligible:1,source_type:'commerce_order',source_id:'order',source_href:'/finance/invoices/invoice?tag=one&tag=two#review',source_title:'Current invoice',title:'Review actual invoice'}],people:[],sourceTypes:[],canWrite:false,canReview:false};
  const html=renderToStaticMarkup(tree(h(CollaborationWorkConsole,{base,data})));expect(html).toContain('href="/api/v1/holdings/acme/collaboration?format=csv"');expect(html).toContain('data-product-switch="true"');const hrefs=[...html.matchAll(/href="([^\"]+)"/g)].map(match=>match[1].replaceAll('&amp;','&'));const source=hrefs.find(href=>href.startsWith('https://apps-dev-erpx.jesadakorn.com/'))!;expect(source).toBeTruthy();const next=new URL(source).searchParams.get('next')!;const target=new URL(next,'https://target.invalid').searchParams.get('next')!;expect(target).toBe(base+'/inbox');expect(html).toContain('data-prefetch="false"');
 });
 it('renders actual assistant settings recovery on Admin before hydration without prefetching SSO',async()=>{
  const {SocialAgentStatus}=await import('@erpx/feature-collaboration/components/social-agent-status');const html=renderToStaticMarkup(tree(h(SocialAgentStatus,{reply:{id:'reply',state:'failed',error:'Configuration needs review',actor_id:'other',requester_name:'Other colleague',attempt:1},api:'/api/v1'+base,userId:'current',onChanged:async()=>{},aiName:'Assistant',settingsUrl:base+'/settings?section=ai#provider'})));expect(html).toContain('https://admin-dev-erpx.jesadakorn.com/sso/start?');expect(html).toContain('data-product-switch="true"');expect(html).toContain('data-prefetch="false"');expect(html).not.toContain('href="'+base+'/administration/');
 });
});
it('hands off only registered personal Leave and Overtime owner pages through Work without interpreting generic People records',()=>{
 const scope:ProductNavigationScope={audience:'pms',companyId:'company-two',companySlug:'acme-co',workspaceBase:base};
 for(const path of ['leave','leave/new','overtime','overtime/new','overtime/actual-overtime/overview','tasks/people_payroll/actual-leave/overview']){const result=productNavigationHref(base+'/'+path+'?tag=one&tag=two#evidence',scope);expect(result.handoff).toBe(true);const sso=new URL(result.href);expect(sso.origin).toBe('https://apps-dev-erpx.jesadakorn.com');const launch=new URL(sso.searchParams.get('next')!,'https://launch.invalid');expect(launch.searchParams.get('companyId')).toBe('company-two');expect(launch.searchParams.get('next')).toBe(base+'/'+path+'?tag=one&tag=two#evidence');}
 for(const path of ['tasks/people_payroll','tasks/people_payroll/new','tasks/people_payroll/actual-leave/private','overtime/actual-overtime/private','leave/private'])expect(productNavigationHref(base+'/'+path,scope).handoff).toBe(false);
 expect(productNavigationHref('/holdings/other/leave',scope).handoff).toBe(false);expect(productNavigationHref(base+'/branches/other/leave',scope).handoff).toBe(false);expect(productNavigationHref('/api/v1'+base+'/leave',scope).handoff).toBe(false);
});


it('keeps product link context free of presentation wrappers and preserves the default Console destination',()=>{
 const child=h('section',{'data-widget':'calendar',className:'legacy-grid',style:{height:'540px'}},h('a',{href:base+'/calendar?view=week#today'},'Calendar'));
 const original=renderToStaticMarkup(child);
 expect(renderToStaticMarkup(h(ProductLinkEnvironment,{audience:null,children:h(ProductWorkLinks,{companyId:'company',workspaceBase:base+'/',children:child})}))).toBe(original);
 expect(renderToStaticMarkup(h(ProductLinkEnvironment,{audience:'pms',children:h(ProductWorkLinks,{companyId:'company',workspaceBase:base,children:child})}))).toBe(original);
 function DefaultLink(){const scope=useProductNavigationScope();expect(scope).toEqual({audience:null});const destination=productNavigationHref(base+'/calendar?view=week#today',scope);expect(destination.handoff).toBe(false);return h(WorkspaceLink,{href:destination.href},'Calendar');}
 const html=renderToStaticMarkup(h(DefaultLink));expect(html).toContain('href="'+base+'/calendar?view=week#today"');expect(html).not.toContain('data-product-switch="true"');
});

import {StandardPmsBackofficeNavigation} from '@erpx/feature-pms/components/standard-pms-backoffice-navigation';
import {StandardPosBackofficeNavigation} from '@erpx/feature-pos/components/standard-pos-backoffice-navigation';
import {pmsMenu,posMenu} from '@erpx/feature-shared/components/hospitality/module-menu';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import type {ComponentProps} from 'react';
const tileButtons=vi.hoisted(()=>({props:[] as ComponentProps<typeof import("@erpx/ui/components/button").Button>[]}));
vi.mock("@erpx/ui/components/button",async importOriginal=>{
 const actual=await importOriginal<typeof import("@erpx/ui/components/button")>();
 return {...actual,Button:(props:ComponentProps<typeof actual.Button>)=>{tileButtons.props.push(props);return h(actual.Button,props);}};
});
function tileLinks(html:string){return [...html.matchAll(/<a\b[^>]*href="([^"]+)"[^>]*>/g)].map(match=>match[1].replaceAll('&amp;','&'));}
it.each(['holding','branch and department'] as const)('renders actual ModuleTiles canonical PMS items on their current %s Front destinations',kind=>{
 const workspaceBase=kind==='holding'?base:base+'/branches/north/departments/desk',location=kind==='holding'?'holding':'branches/north/departments/desk';
 const query='?property=original-hotel&tag=one&tag=two#hotel-work';
 // This is the original PMS Backoffice caller's canonical item projection.
 const items=pmsMenu('en').map(item=>({...item,href:canonicalUiHref(`${workspaceBase}/pms/${item.id}${query}`)}));
 const child=h(StandardPmsBackofficeNavigation,{items,current:'dashboard',label:'Hotel tasks'}),tree=h(ProductLinkEnvironment,{audience:'pms',children:h(ProductWorkLinks,{companyId:'current-company',companySlug:'acme-co',workspaceBase,children:child})});
 const html=renderToStaticMarkup(tree),hrefs=tileLinks(html),daily=['dashboard','reservations','front-desk','rooms','housekeeping','guest-accounts','cashier','night-audit'];
 expect(hrefs).toHaveLength(items.length);
 for(const item of items){
  const expected=daily.includes(item.id)?`/acme/acme-co/${location}/${item.id==='dashboard'?'today':item.id}${query}`:`${workspaceBase}/pms/${item.id}${query}`;
  expect(hrefs, item.id).toContain(expected);
 }
 expect(hrefs.filter(href=>href.startsWith('/acme/acme-co/'))).toHaveLength(8);
 expect(hrefs.filter(href=>href.startsWith(workspaceBase+'/pms/'))).toHaveLength(4);
 expect(html).toContain('aria-current="page"');expect(html).not.toContain('data-product-switch="true"');expect(html).not.toContain('/sso/start');
});
it('keeps actual ModuleTiles native Front links local with duplicate selectors and fragments',()=>{
 const query='?property=original-hotel&tag=one&tag=two#hotel-work',items=pmsMenu('en').filter(item=>['dashboard','rooms','guest-accounts'].includes(item.id)).map(item=>({...item,href:`/acme/acme-co/holding/${item.id==='dashboard'?'today':item.id}${query}`}));
 const child=h(StandardPmsBackofficeNavigation,{items,current:'rooms',label:'Hotel tasks'}),html=renderToStaticMarkup(h(ProductLinkEnvironment,{audience:'pms',children:h(ProductWorkLinks,{companyId:'current-company',companySlug:'acme-co',workspaceBase:base,children:child})}));
 expect(tileLinks(html)).toEqual(items.map(item=>item.href));expect(html).not.toContain('/sso/start');expect(html).not.toContain('data-product-switch="true"');
});
it('retains actual POS ModuleTiles selection callbacks instead of manufacturing hrefs',()=>{
 tileButtons.props.length=0;
 const onSelect=vi.fn(),items=posMenu('en').filter(item=>['sale','kitchen','shifts','history'].includes(item.id)),html=renderToStaticMarkup(h(StandardPosBackofficeNavigation,{items,current:'sale',label:'Point of sale',onSelect}));
 expect(tileLinks(html)).toEqual([]);expect(onSelect).not.toHaveBeenCalled();
 const history=tileButtons.props.find(props=>props['aria-label']==='History');
 expect(history?.type).toBe('button');expect(history?.onClick).toBeTypeOf('function');
 Reflect.apply(history!.onClick!,undefined,[]);
 expect(onSelect.mock.calls).toEqual([['history']]);
 expect(tileButtons.props.find(props=>props['aria-label']==='Sale')?.['aria-current']).toBe('page');
});


describe("Grouped product sidebars", () => {
 it("partitions all eligible products exactly once and keeps the caller's original handoffs", () => {
  const entries = products.map(product => ({id:product.id,productId:product.id,label:product.label.en,href:`https://${product.id}.example/launch?company=original#scope`}));
  const model = groupAppsSidebar([{id:'applications',label:'All apps',href:'/applications'},...entries,{id:'account',label:'Account',href:'/account'},entries[0]], 'en');
  expect(model.primary.map(item=>item.id)).toEqual(['applications','apps']);
  const grouped = model.groups.flatMap(group=>group.items);
  expect(new Set(grouped.map(item=>item.id)).size).toBe(products.length-1);
  expect(grouped).toHaveLength(products.length-1);
  expect([...model.primary.filter(item=>item.productId),...grouped].map(item=>item.href).sort()).toEqual(entries.map(item=>item.href).sort());
  expect(model.groups.find(group=>group.id==='services')?.items.map(item=>item.id)).toEqual(['assets','service','legal']);
  expect(model.groups.find(group=>group.id==='customers-sales')?.items.map(item=>item.id)).toEqual(['sales','support']);
  expect(model.account?.href).toBe('/account');
 });
 it("omits empty categories and never synthesizes an unauthorized product or Provider destination", () => {
  const model = groupAppsSidebar([{id:'finance',productId:'finance',label:'การเงิน',href:'/allowed'}], 'th');
  expect(model.primary).toEqual([]);
  expect(model.groups.map(group=>[group.label,group.items.map(item=>item.href)])).toEqual([['การเงิน',['/allowed']]]);
  expect(groupAppsSidebar([], 'en').groups).toEqual([]);
  expect(groupAppsSidebar([{id:'provider',groupId:'administration',label:'Provider',href:'/provider'}], 'en').groups[0].items[0].href).toBe('/provider');
 });
 it("preserves native Hotel property hrefs and partitions only owner-filtered hospitality actions", () => {
  for (const app of ['pos','pms'] as const) {
   const menu = (app==='pos'?posMenu('en'):pmsMenu('en')).map(item=>({...item,href:`/native/${item.id}?property=selected`}));
   const visible = menu.filter((_, index)=>index%2===0);
   const groups = groupHospitalitySidebar(app, visible, 'th');
   const entries = groups.flatMap(group=>group.items);
   expect(entries.map(item=>item.id).sort()).toEqual(visible.map(item=>item.id).sort());
   expect(new Set(entries.map(item=>item.id)).size).toBe(entries.length);
   expect(entries.map(item=>item.href).sort()).toEqual(visible.map(item=>item.href).sort());
   expect(groups.every(group=>group.items.length>0)).toBe(true);
  }
 });
 it("selects the exact visible child page and opens its authorized category", () => {
  for (const [productId,path,expected] of [['finance','finance/documents','finance-sales'],['sales','customers-sales/sales/pipeline','crm-pipeline'],['learn','knowledge/learning','learning'],['people','people-organization/people/directory/current','people-directory'],['assets','services/assets/register','assets']] as const) {
   const model=workplaceNavigation({workspaceBase:base,productId,role:'holding_owner',moduleIds:products.find(product=>product.id===productId)!.modules,financeWorkspaceV2:true});
   const active=resolveNavigation(model,base,base+'/'+path),selected=sidebarSelection(model,active,base+'/'+path);
   expect(selected.itemId).toBe(expected);expect(selected.categoryId).toBeTruthy();
  }
  const model=workplaceNavigation({workspaceBase:base,productId:'apps',role:'holding_owner'});
  expect(sidebarSelection(model,resolveNavigation(model,base,base+'/home'),base+'/home')).toEqual({itemId:null,categoryId:null});
 });
 it("opens the active category before a saved choice and tolerates obsolete or missing storage", () => {
  expect(sidebarInitialGroup(['people','finance'],'finance','people')).toBe('finance');
  expect(sidebarInitialGroup(['people','finance'],null,'finance')).toBe('finance');
  expect(sidebarInitialGroup(['people','finance'],null,'retired')).toBe('people');
  expect(sidebarInitialGroup(['people','finance'],null,null)).toBe('people');
  expect(sidebarInitialGroup(['people','finance'],null,'')).toBeNull();
  expect(sidebarInitialGroup([],null,'finance')).toBeNull();
 });
 it("renders unique disclosure targets with hidden children and an accessible active link", () => {
  const section = (label:string,open:boolean) => h(SidebarSection,{label,open,icon:h('svg'),onToggle:()=>{},children:h('a',{href:'/original?property=h1','aria-current':'page'},'Original destination')});
  const html=renderToStaticMarkup(h('nav',null,section('Finance',true),section('People',false)));
  const controls=[...html.matchAll(/aria-controls="([^"]+)"/g)].map(match=>match[1]);
  expect(controls).toHaveLength(2);expect(new Set(controls).size).toBe(2);
  for(const id of controls)expect(html).toContain(`id="${id}"`);
  expect(html).toContain('aria-expanded="false"');expect(html).toContain('hidden=""');
  expect(html).toContain('aria-current="page"');expect(html).toContain('/original?property=h1');
 });
});

it("separates relationship discovery from account administration",()=>{
 const hr=workplaceNavigation({workspaceBase:base,role:"hr_admin"}), it=workplaceNavigation({workspaceBase:base,role:"it_admin"});
 for(const model of [hr,it])for(const id of ["people-external","people-unclassified"])expect(model.search.some(item=>item.id===id)).toBe(true);
 expect(hr.search.some(item=>item.id==="technology-accounts")).toBe(false);
 expect(it.search.some(item=>item.id==="technology-accounts")).toBe(true);
});

it("retains selected subjects, repeated filters and fragments when legacy People/IAM links move",()=>{
 for(const [from,to] of [["people?view=users&section=users","administration/technology/resources/accounts"],["people-organization/people/users-access/users","administration/technology/resources/accounts"],["people-organization/people/users-access/roles","administration/technology/settings/roles"],["people-organization/people/users-access/teams","people-organization/people/external/teams"],["iam/users","administration/technology/resources/accounts"]]){
  const url=new URL(canonicalUiHref(`${base}/${from}${from.includes("?")?"&":"?"}subject=account-one&q=alex&status=active&filter=one&filter=two#details`),"https://test.invalid");
  expect(url.pathname).toBe(`${base}/${to}`);expect(url.searchParams.get("subject")).toBe("account-one");expect(url.searchParams.get("q")).toBe("alex");expect(url.searchParams.getAll("filter")).toEqual(["one","two"]);expect(url.hash).toBe("#details");
 }
});

it("keeps external team filters on the relationship owner instead of opening an employee team",()=>{
 expect(canonicalUiHref(`${base}/people?view=external&section=users&team=vendor-team&q=alex&status=active`)).toBe(`${base}/people-organization/people/external/users?team=vendor-team&q=alex&status=active`);
});
