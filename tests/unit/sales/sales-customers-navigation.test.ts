import { expect, it } from "vitest";
import { hubHref, salesCustomerAccess, salesCustomerLegacyDestination } from "@erpx/server-sales/public/rules/navigation";
import { hubResultHref } from "@erpx/feature-shared/lib/hub-result";
import { workplaceNavigation, navigationTrail, resolveNavigation } from "@erpx/feature-work_projection/lib/workplace-navigation";
import { navigationLauncher } from "@erpx/feature-work_projection/lib/navigation-catalog";
import { productionPageCut } from "@erpx/feature-shared/lib/hospitality-page-cut";
const base = "/holdings/acme/branches/main";

it("moves list routes while retaining filters, cursor and creation context", () => {
  const query = { q: "Harbor & sons", cursor: "cursor==", new: "1", edit: "customer-1" };
  const url = new URL(salesCustomerLegacyDestination(base, "commerce", { ...query, view: "quote" })!, "https://example.test");
  expect(url.pathname).toBe(`${base}/customers-sales/sales/documents/quote`);
  expect(Object.fromEntries(url.searchParams)).toEqual(query);
  expect(salesCustomerLegacyDestination(base, "commerce", { view: "purchase" })).toBeNull();
  expect(salesCustomerLegacyDestination(base, "commerce", { view: "parties", kind: "supplier" })).toBeNull();
  expect(salesCustomerLegacyDestination(base, "crm/lead-1", {})).toBeNull();
  const source = new URL(salesCustomerLegacyDestination(base, "support", { sourceKind: "asset", sourceId: "asset-1", sourceVersion: "3" })!, "https://example.test");
  expect(source.searchParams.get("new")).toBe("1");
  expect(source.searchParams.get("sourceVersion")).toBe("3");
});
it("opens saved owner records in the active list without losing filters", () => {
  const result = new URL(hubResultHref(`https://example.test${base}/sales?view=documents&type=quote&q=Harbor&new=1`, base, `${base}/customers-sales/sales/documents/quote/order-1/overview`)!, "https://example.test");
  expect(result.pathname).toBe(`${base}/customers-sales/sales/documents/quote/order-1/overview`);
  expect(result.searchParams.get("returnTo")).toBe(`${base}/customers-sales/sales/documents/quote?q=Harbor`);
  expect(result.searchParams.get("new")).toBeNull();
  expect(result.searchParams.get("detailKind")).toBeNull();
  expect(result.searchParams.get("detailId")).toBeNull();
  expect(hubResultHref(`https://example.test${base}/crm/lead-1`, base, `${base}/customers-sales/sales/pipeline/lead-2/overview`)).toBeNull();
  expect(hubHref(base, "customers", { q: "A", cursor: "old" }, { cursor: undefined, view: "support" })).toBe(`${base}/customers-sales/customers/service/cases?q=A`);
  expect(new URL(hubHref(base, "customers", { view: "reviews", status: "" }, { review: "duplicates" }), "https://example.test").searchParams.get("status")).toBe("");
});
it("shows two workspaces and keeps every auxiliary task searchable", () => {
  const model = workplaceNavigation({ workspaceBase: base, role: "holding_owner", moduleIds: ["marketing", "ecommerce"] });
  const category = model.categories.find(category => category.id === "customers-sales")!;
  expect(category.apps.map(item => item.label)).toEqual(["Sales", "Customers"]);
  expect(category.secondaryItems?.map(item => item.id)).toEqual(["marketing", "ecommerce"]);
  for (const text of ["Follow-ups", "Pipeline", "Quotations", "Price lists", "Customer access", "Support cases", "Marketing", "Channels", "Contacts", "reports"]) expect(navigationLauncher(model, text).results.length, text).toBeGreaterThan(0);
  expect(resolveNavigation(model, base, `${base}/customers-sales/sales/pipeline/lead-1/overview`).sidebarId).toBe("sales-hub");
  expect(resolveNavigation(model, base, `${base}/customers-sales/customers/service/cases/ticket-1/overview`).sidebarId).toBe("customers-hub");
  expect(navigationTrail(model, base, `${base}/customers-sales/sales/pipeline/lead-1/overview`).map(item => item.label)).toEqual(["Customers & sales", "Sales", "Pipeline", "Lead 1", "Overview"]);
});
it("allows Support-only entry without registry or sales permission", () => {
  const actor = { role: "employee", permissions: ["support.read"] }, access = salesCustomerAccess(actor, ["support"]);
  expect(access).toMatchObject({ support: true, directory: false, contacts: false, today: false, documents: false, reviews: false });
  const model = workplaceNavigation({ workspaceBase: base, permissions: ["support.read"], availableAppIds: ["support"] });
  expect(model.categories.find(category => category.id === "customers-sales")?.apps).toMatchObject([{ id: "customers-hub", href: `${base}/customers-sales/customers/service/cases` }]);
  expect(model.search.some(item => item.id === "crm-pipeline")).toBe(false);
});
it("checks each production view against its source rollout", () => {
  const catalogue = [{ id: "crm", name: "CRM", state: "Coming Soon" as const }, { id: "sales", name: "Sales", state: "Coming Soon" as const }, { id: "support", name: "Support", state: "Pilot" as const }];
  expect(productionPageCut(`${base}/customers-sales/customers`, base, new URLSearchParams(), catalogue)).toBeNull();
  expect(productionPageCut(`${base}/customers-sales/customers`, base, new URLSearchParams("view=support"), catalogue)).toBeNull();
  expect(productionPageCut(`${base}/customers-sales/sales/pipeline`, base, new URLSearchParams(), catalogue)).toMatchObject({ id: "crm" });
  expect(salesCustomerAccess({ role: "employee", permissions: ["commerce.read", "commerce.sales"], branchId: "branch" }).pricing).toBe(false);
});

// Actual record-origin link and actual async CustomerSupportView body. Only
// selected owner readers/presentation leaves are ports; no native authority or
// database acceptance is inferred from these SSR interaction cases.
import * as SourceReportReact from 'react';
import {renderToStaticMarkup as renderSourceReport} from 'react-dom/server';
import {vi} from 'vitest';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {AuthorizedSourceRecord} from '@erpx/server-records/public/source-record-policy';
import type {CustomerSupportView as ActualCustomerSupportView} from '@erpx/feature-sales/components/customers/customer-views';
import type {SourceSupportLink as ActualSourceSupportLink} from '@erpx/feature-support/components/source-support-link';
import {queryValue} from '@erpx/server-sales/public/rules/navigation';
type ReportOrigin=Parameters<typeof ActualSourceSupportLink>[0];
type ReportViewProps=Parameters<typeof ActualCustomerSupportView>[0];
function sourceReportActor():WorkContext{return{userId:'reporter',name:'Reporter',email:'reporter@example.test',assignmentId:'current-selected-assignment',role:'employee',companyId:'company',branchId:'branch',departmentId:null,holdingId:'H001',holdingSlug:'acme',holdingName:'Acme',scope:{kind:'branch',holdingId:'H001',holdingSlug:'acme',holdingName:'Acme',companyId:'company',branchId:'branch',branchSlug:'main',branchName:'Main'},assignmentScope:'branch',inherited:false,modules:['support'],assignments:[],workspaces:[]};}
async function actualSourceReportLink(input:ReportOrigin){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),text=await readFile(new URL("../../../packages/feature-support/src/components/source-support-link.tsx",import.meta.url),'utf8'),tree=ts.createSourceFile('actual-source-support-link.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),owner=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='SourceSupportLink');if(!owner)throw Error('Actual SourceSupportLink required');
 const emitted=ts.transpileModule(`${owner.getText(tree).replace(/^export\s+/,'')}\nreturn SourceSupportLink;`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText,Component=new Function('React','Link','canonicalUiHref',emitted)(SourceReportReact,'a',canonicalUiHref) as (props:ReportOrigin)=>SourceReportReact.ReactElement<{href:string}>;
 return Component(input);
}
function sourceReportNodes(node:SourceReportReact.ReactNode,out:SourceReportReact.ReactElement<Record<string,unknown>>[]=[]){SourceReportReact.Children.forEach(node,child=>{if(!SourceReportReact.isValidElement<Record<string,unknown>>(child))return;out.push(child);sourceReportNodes(child.props.children as SourceReportReact.ReactNode,out);});return out;}
async function actualSourceReportView(readSource:(actor:WorkContext,input:{kind:string;id:string;version:number})=>Promise<AuthorizedSourceRecord>,canCreate=true){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),text=await readFile(new URL("../../../packages/feature-sales/src/components/customers/customer-views.tsx",import.meta.url),'utf8'),tree=ts.createSourceFile('actual-source-report-view.tsx',text,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),owner=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='CustomerSupportView');if(!owner)throw Error('Actual CustomerSupportView required');
 const supportWorkspace=vi.fn(async()=>({items:[],sources:[],canCreate,canCreateOrder:false,ownReportsOnly:true})),supportRecipients=vi.fn(async()=>[]),readSourceRecord=vi.fn(readSource),styles={filterRow:'filters',section:'section',actions:'actions'},ports={React:SourceReportReact,supportWorkspace,supportRecipients,queryValue,readSourceRecord,hubPage:(rows:unknown[])=>rows,styles,HubFilters:'Filters',HubCreate:'CreateDrawer',Field:'label',Select:'select',Button:'button',SupportConsole:'SupportConsole',HubTable:'table',Link:'a',StatusBadge:'Status',HubEmpty:'Empty',HubPagination:'Pagination',ActionLink:'a',canonicalUiHref},emitted=ts.transpileModule(`${owner.getText(tree).replace(/^export\s+/,'')}\nreturn CustomerSupportView;`,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText,Component=new Function(...Object.keys(ports),emitted)(...Object.values(ports)) as (props:ReportViewProps)=>Promise<SourceReportReact.ReactNode>;
 return{read:Component,supportWorkspace,supportRecipients,readSourceRecord};
}
it.each(['pos_sale','pms_record','asset'] as const)('the actual %s record Report link opens canonical creation with its exact original source identity/version',async kind=>{
 const link=await actualSourceReportLink({base,kind,id:'record-42',version:7,label:'Report this exact record'}),url=new URL(link.props.href,'https://helpdesk.example.test');expect(url.pathname).toBe(`${base}/customers-sales/customers/service/cases`);expect(Object.fromEntries(url.searchParams)).toEqual({sourceKind:kind,sourceId:'record-42',sourceVersion:'7',new:'1'});expect(renderSourceReport(link)).toContain('Report this exact record');
 const legacy=new URL(salesCustomerLegacyDestination(base,'support',{sourceKind:kind,sourceId:'record-42',sourceVersion:'7'})!,'https://helpdesk.example.test');expect(url.pathname).toBe(legacy.pathname);expect(Object.fromEntries(url.searchParams)).toEqual(Object.fromEntries(legacy.searchParams));
 const actor:WorkContext={...sourceReportActor(),modules:['support',kind==='pos_sale'?'pos':kind==='pms_record'?'pms':'assets']},source:AuthorizedSourceRecord={kind,id:'record-42',version:7,title:'Authorized current source',href:'/source/record-42',label:'Native authorized source',companyId:actor.companyId,branchId:actor.branchId,departmentId:null,status:'open'},view=await actualSourceReportView(async()=>source),query=Object.fromEntries(url.searchParams),tree=await view.read({actor,base,query}),nodes=sourceReportNodes(tree),consoleElement=nodes.find(node=>node.type==='SupportConsole'),drawer=nodes.find(node=>node.type==='CreateDrawer');
 expect(view.supportWorkspace).toHaveBeenCalledExactlyOnceWith(actor);expect(view.readSourceRecord).toHaveBeenCalledExactlyOnceWith(actor,{kind,id:'record-42',version:7});expect(view.supportRecipients).not.toHaveBeenCalled();expect(drawer).toBeDefined();expect(consoleElement?.props.typedSource).toEqual(source);expect(consoleElement?.props.source).toBeUndefined();expect(consoleElement?.props.data).toMatchObject({canCreate:true,canCreateOrder:false,ownReportsOnly:true});expect(actor.modules).not.toContain('people_payroll');
});
it.each([[403,'MODULE_FORBIDDEN'],[404,'SOURCE_RECORD_NOT_FOUND'],[409,'SOURCE_RECORD_CHANGED']] as const)('opening the actual report drawer never swallows native source refusal %s/%s',async(status,code)=>{
 const actor=sourceReportActor(),failure=Object.assign(new Error('Current source denied'),{status,code}),link=await actualSourceReportLink({base,kind:'asset',id:'current-asset',version:9}),url=new URL(link.props.href,'https://helpdesk.example.test'),view=await actualSourceReportView(async()=>{throw failure;});await expect(view.read({actor,base,query:Object.fromEntries(url.searchParams)})).rejects.toBe(failure);expect(view.readSourceRecord).toHaveBeenCalledExactlyOnceWith(actor,{kind:'asset',id:'current-asset',version:9});
});
it('an ordinary case list still performs no source handoff and inactive creation capability renders no report console',async()=>{
 const unused=vi.fn(async():Promise<AuthorizedSourceRecord>=>{throw Error('No record should be selected');}),view=await actualSourceReportView(unused),actor=sourceReportActor(),list=await view.read({actor,base,query:{view:'support'}});expect(sourceReportNodes(list).some(node=>node.type==='SupportConsole')).toBe(false);expect(view.readSourceRecord).not.toHaveBeenCalled();
 const inactive=await actualSourceReportView(unused,false),create=await inactive.read({actor,base,query:{view:'support',new:'1'}});expect(sourceReportNodes(create).some(node=>node.type==='SupportConsole')).toBe(false);expect(inactive.readSourceRecord).not.toHaveBeenCalled();
});
