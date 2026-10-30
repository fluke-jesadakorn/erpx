vi.mock('server-only',()=>({}));
const menuSources=vi.hoisted(()=>({products:vi.fn(),admin:vi.fn()}));
vi.mock("@erpx/server-work_projection/public/product-launcher",()=>({productLaunchChoices:menuSources.products}));
vi.mock("@erpx/server-work_projection/public/admin-launcher",()=>({adminLaunchChoices:menuSources.admin}));
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import { expect, it, vi } from "vitest";
import { buildWorkspaceHoldingChoice, type WorkspaceChoice } from "@erpx/feature-work_projection/lib/workspace-choice";
import { createProductLauncher, productWorkspaceCandidates } from "@erpx/server-work_projection/application/product-launcher";
import type { AccountAccessContext } from "@erpx/server-identity_access/public/authorization";
import type { CompanyPackageGrant, CompanyPackageHistoryRight } from "@erpx/server-commercial/public/packages";
import { builtInPermissions } from "@erpx/feature-identity_access/lib/permissions";
import { productOfficeLanding, productDestinationAvailable } from "@erpx/server-work_projection/domain/product-landings";
import { commercialPackages } from "@erpx/server-commercial/domain/packages";
import {developmentModuleAdmissionDeferred} from '@erpx/contracts/development-module-admission';
import {productIds} from '@erpx/contracts/product-registry';

const choices = (): WorkspaceChoice[] => [buildWorkspaceHoldingChoice("acme", "Acme Holding", [
  { companyId: "c1", companyName: "Company One", scopeKind: "holding", scopeNodeId: null },
  { companyId: "c2", companyName: "Company Two", scopeKind: "department", scopeNodeId: "d2" },
], [
  { id: "b1", kind: "branch", slug: "branch-one", label: "Branch One", companyId: "c1", branchId: null, branchSlug: null },
  { id: "d1", kind: "department", slug: "department-one", label: "Department One", companyId: "c1", branchId: "b1", branchSlug: "branch-one" },
  { id: "b2", kind: "branch", slug: "branch-two", label: "Branch Two", companyId: "c2", branchId: null, branchSlug: null },
  { id: "d2", kind: "department", slug: "department-two", label: "Department Two", companyId: "c2", branchId: "b2", branchSlug: "branch-two" },
], true)!];
const grant = (companyId: string, packageId = "finance"): CompanyPackageGrant => ({ companyId, packageId, packageVersion: 1, state: "active", startsAt: 0, endsAt: null, version: 1, reason: "Fixture", evidence: "fixture" });
function fixture() {
  const access = vi.fn(async (scope, options) => ({ scope: { ...scope, holdingId: "h001", companyId: options.companyId, branchId: scope.kind !== "holding" ? scope.branchSlug === "branch-one" ? "b1" : "b2" : undefined, departmentId: scope.kind === "department" ? scope.departmentSlug === "department-one" ? "d1" : "d2" : undefined },
    assignments: [{ id: "role", role: "holding_owner", companyId: options.companyId, branchId: null, scopeKind: "holding", scopeNodeId: null, modules: ["finance", "pos"] }] }) as AccountAccessContext);
  const grants = vi.fn(async (access: AccountAccessContext) => [grant(access.scope.companyId)]);
  const history = vi.fn<(access: AccountAccessContext) => Promise<CompanyPackageHistoryRight[]>>(async () => []);
  const purchaseFacts = vi.fn(async (access: AccountAccessContext) => ({grants:await grants(access),history:await history(access)}));
  const ready = vi.fn(async () => true);
  const moduleAccess = vi.fn(async () => ({ modulesByAssignment: new Map([["role", ["finance", "pos"]]]),
    presentationModulesByAssignment: new Map([["role", ["finance", "pos"]]]),
    permissionsByAssignment: new Map([["role", builtInPermissions("holding_owner")]]), beta: false }));
  const ports = { choices: async () => choices(), access, purchaseFacts, grants, history, ready, moduleAccess, companySlug: async () => "company-one", origin: (id: string) => `https://${id}-dev-erpx.jesadakorn.com`, denied: (error: unknown) => error instanceof Error && error.message === "FORBIDDEN", now: () => 1000 };
  return { ...ports, launch: () => createProductLauncher(ports)() };
}
it('defers only Commercial admission at the exact authorized development cutover',()=>{
 const environment={ERPX_ENVIRONMENT:'local',ERPX_PRODUCT_ENVIRONMENT:'development',ERPX_PUBLIC_ORIGIN:'https://console-dev-erpx.jesadakorn.com',ERPX_PRODUCT_ROUTING:'enabled',ERPX_PRODUCT_SSO:'enabled',ERPX_CONSOLE_RETIRED:'enabled'};
 for(const id of productIds)expect(developmentModuleAdmissionDeferred(id,environment)).toBe(true);
 for(const key of Object.keys(environment))expect(developmentModuleAdmissionDeferred('finance',{...environment,[key]:'other'})).toBe(false);
 for(const mode of ['shadow','enforced','invalid'])expect(developmentModuleAdmissionDeferred('finance',{...environment,ERPX_FINANCE_PACKAGE_ENFORCEMENT:mode})).toBe(false);
 expect(developmentModuleAdmissionDeferred('finance',{...environment,ERPX_ENVIRONMENT:'production'})).toBe(false);
 expect(developmentModuleAdmissionDeferred('sales',{...environment,ERPX_COMMERCE_PACKAGE_ENFORCEMENT:'enforced'})).toBe(false);
 expect(developmentModuleAdmissionDeferred('supply',{...environment,ERPX_COMMERCE_PACKAGE_ENFORCEMENT:'enforced'})).toBe(false);
 expect(developmentModuleAdmissionDeferred('unknown' as typeof productIds[number],environment)).toBe(false);
});
it('keeps existing Finance module entry without manufacturing grants, and retains readiness and current role permissions',async()=>{
 const f=fixture();f.grants.mockResolvedValue([]);
 const launch=createProductLauncher({...f,commercialAdmissionDeferred:id=>id==='finance'});
 const original=structuredClone(await f.moduleAccess());
 const links=await launch('finance');expect(links).toHaveLength(4);
 expect(links.every(link=>new URL(link.href).origin==='https://finance-dev-erpx.jesadakorn.com')).toBe(true);
 expect(await f.moduleAccess()).toEqual(original);
 expect(await f.purchaseFacts(await f.access({kind:'holding',holdingSlug:'acme'},{companyId:'c1'}))).toEqual({grants:[],history:[]});
 f.ready.mockResolvedValue(false);expect(await launch('finance')).toEqual([]);
 f.ready.mockResolvedValue(true);f.moduleAccess.mockResolvedValue({...original,permissionsByAssignment:new Map([['role',[]]])});expect(await launch('finance')).toEqual([]);
 f.moduleAccess.mockResolvedValue({...original,modulesByAssignment:new Map(),presentationModulesByAssignment:new Map()});expect(await launch('finance')).toEqual([]);
});
it("enumerates every Company and inherited Branch/Department without promoting a direct Department to its parent", () => {
  const candidates = productWorkspaceCandidates(choices());
  expect(candidates.map(candidate => `${candidate.companyId}/${candidate.scope.kind}`)).toEqual(["c1/holding", "c1/branch", "c1/department", "c2/department"]);
  expect(productWorkspaceCandidates([{ kind: "platform", label: "Platform", destination: "/platform" }])).toEqual([]);
});
it("checks explicit Company authority for every scope and caches only the grant read per Company in one render", async () => {
  const f = fixture(), links = await f.launch();
  expect(links).toHaveLength(4);
  expect(f.access.mock.calls.map(([, options]) => options.companyId)).toEqual(["c1", "c1", "c1", "c2"]);
  expect(f.grants).toHaveBeenCalledTimes(2);
  const target = new URL(new URL(links[3].href).searchParams.get("next")!, "https://finance-dev-erpx.jesadakorn.com");
  expect(target.pathname).toBe("/launch");
  expect(target.searchParams.get("companyId")).toBe("c2");
  expect(target.searchParams.get("next")).toBe("/holdings/acme/branches/branch-two/departments/department-two/finance/documents");
});
it("hides unpurchased, release-denied and revoked destinations and surfaces unexpected authority failures", async () => {
  const f = fixture();
  f.grants.mockResolvedValue([]); expect(await f.launch()).toEqual([]);
  f.grants.mockImplementation(async access => [grant(access.scope.companyId)]);
  f.ready.mockResolvedValue(false); expect(await f.launch()).toEqual([]);
  f.ready.mockResolvedValue(true);
  f.access.mockRejectedValue(new Error("FORBIDDEN")); expect(await f.launch()).toEqual([]);
  f.access.mockRejectedValue(new Error("DATABASE_UNAVAILABLE")); await expect(f.launch()).rejects.toThrow("DATABASE_UNAVAILABLE");
});
it("keeps draining and expired products reachable in every authorized scope for existing work", async () => {
  for(const patch of [{state:"draining" as const},{endsAt:1000}]){
    const f=fixture();f.grants.mockImplementation(async access=>[{...grant(access.scope.companyId),...patch}]);
    const links=await f.launch();expect(links).toHaveLength(4);
    expect(links.every(link=>link.purchasePurpose==='settle-existing')).toBe(true);
    expect(f.purchaseFacts).toHaveBeenCalledTimes(2);expect(f.history).toHaveBeenCalledTimes(2);
  }
});
it("offers retained history after inactivation or future renewal without exposing private purchase facts", async () => {
  for(const patch of [{state:"inactive" as const},{startsAt:2000}]){
    const f=fixture();f.grants.mockImplementation(async access=>[{...grant(access.scope.companyId),...patch}]);
    f.history.mockImplementation(async access=>[{companyId:access.scope.companyId,packageId:'finance',packageVersion:1,sourceGrantVersion:1,sourceEventId:'private-history-event',retainedAt:0}]);
    const links=await f.launch();expect(links).toHaveLength(4);expect(links.every(link=>link.purchasePurpose==='history')).toBe(true);
    expect(JSON.stringify(links)).not.toContain('private-history-event');expect(JSON.stringify(links)).not.toMatch(/sourceGrantVersion|retainedAt|"permissions"|"grants"/);
  }
});
it("does not infer history from an unstarted purchase, a bare inactive row or another Company's retained right", async () => {
  for(const patch of [{startsAt:2000},{state:"inactive" as const}]){
    const f=fixture();f.grants.mockImplementation(async access=>[{...grant(access.scope.companyId),...patch}]);
    expect(await f.launch()).toEqual([]);
    f.history.mockResolvedValue([{companyId:'foreign',packageId:'finance',packageVersion:1,sourceGrantVersion:1,sourceEventId:'foreign-history',retainedAt:0}]);
    expect(await f.launch()).toEqual([]);expect(f.moduleAccess).not.toHaveBeenCalled();
  }
});
it("labels the authorized role's actual entry capability and prefers an eligible new-work role without selecting it", async () => {
  const f = fixture();
  f.grants.mockImplementation(async access => [grant(access.scope.companyId, "sales"), { ...grant(access.scope.companyId, "sales-marketing"), state: "inactive" }]);
  f.history.mockImplementation(async access => [{ companyId: access.scope.companyId, packageId: "sales-marketing", packageVersion: 1,
    sourceGrantVersion: 1, sourceEventId: "retained-marketing-event", retainedAt: 0 }]);
  f.moduleAccess.mockResolvedValue({ modulesByAssignment: new Map([["role", ["marketing"]]]),
    presentationModulesByAssignment: new Map([["role", ["marketing"]]]), permissionsByAssignment: new Map([["role", ["records.read"]]]), beta: false });
  const historical = await f.launch();
  expect(historical).toHaveLength(4);
  expect(historical.every(link => link.productId === "sales" && link.purchasePurpose === "history")).toBe(true);
  const originalAccess = f.access.getMockImplementation()!;
  f.access.mockImplementation(async (scope, options) => {
    const access = await originalAccess(scope, options);
    return { ...access, assignments: [...access.assignments, { ...access.assignments[0], id: "sales-role", role: "sales_operator", modules: ["sales"] }] };
  });
  f.moduleAccess.mockResolvedValue({ modulesByAssignment: new Map([["role", ["marketing"]], ["sales-role", ["sales"]]]),
    presentationModulesByAssignment: new Map([["role", ["marketing"]], ["sales-role", ["sales"]]]),
    permissionsByAssignment: new Map([["role", ["records.read"]], ["sales-role", builtInPermissions("sales_operator")]]), beta: false });
  const available = await f.launch();
  expect(available).toHaveLength(4);
  expect(available.every(link => link.purchasePurpose === "new-work")).toBe(true);
  expect(available.every(link => !link.href.includes("sales-role"))).toBe(true);
});
it("opens a Front Company/location landing directly without a shared Company cookie", async () => {
  const f = fixture(); f.grants.mockImplementation(async access => [grant(access.scope.companyId, "pos")]);
  const links = await f.launch();
  expect(new URL(links[1].href).searchParams.get("next")).toBe("/acme/company-one/branches/branch-one/");
});
it("uses the owner's module projection rather than stale assignment modules", async () => {
  const f = fixture(); f.moduleAccess.mockResolvedValue({ modulesByAssignment: new Map([["role", []]]),
    presentationModulesByAssignment: new Map([["role", []]]), permissionsByAssignment: new Map([["role", []]]), beta: false });
  expect(await f.launch()).toEqual([]);
});
it("keeps purchased personal Work available to an applicable role without granting a business module", async () => {
  const f = fixture(); f.grants.mockImplementation(async access => [grant(access.scope.companyId, "work")]);
  f.moduleAccess.mockResolvedValue({ modulesByAssignment: new Map([["role", []]]),
    presentationModulesByAssignment: new Map([["role", []]]), permissionsByAssignment: new Map([["role", []]]), beta: true });
  const links = await f.launch(); expect(links).toHaveLength(4); expect(links.every(link => link.productId === "work")).toBe(true);
});
it("chooses owner menu landings for narrow roles rather than a forbidden default", () => {
  const capabilities = commercialPackages.flatMap(definition => [...definition.visibleCapabilities, ...definition.includedCapabilities]);
  const check = (product: Parameters<typeof productOfficeLanding>[0], modules: string[], permissions: string[]) =>
    productOfficeLanding(product, { role: "custom_fixture", modules, readyModules: modules, permissions, scopeKind: "holding", capabilities });
  expect(check("sales", ["marketing"], ["records.read"])).toBe("customers-sales/marketing");
  expect(check("supply", ["quality"], ["quality.read"])).toBe("inventory-quality/quality");
  expect(check("people", ["people_payroll"], ["payroll.read"])).toBe("people-organization/people/payroll/dashboard");
  expect(check("manufacturing", ["planning"], ["records.read"])).toBe("production/planning");
  expect(check("assets", ["maintenance"], ["records.read"])).toBe("services/assets/maintenance");
  expect(check("learn", ["learning"], ["records.read"])).toBe("knowledge/learning");
  expect(check("projects", ["projects"], ["records.read"])).toBe("projects-routines/projects");
  expect(check("sales", ["marketing"], [])).toBeNull();
  expect(check("supply", ["quality"], [])).toBeNull();
});
it("does not turn a purchased base into its unpurchased Marketing or Quality add-on", () => {
  expect(productOfficeLanding("sales", { role: "holding_owner", modules: ["marketing"], readyModules: ["marketing"], permissions: ["records.read"],
    scopeKind: "holding", capabilities: ["sales.crm", "sales.customers", "sales.quotes", "sales.orders"] })).toBeNull();
  expect(productOfficeLanding("supply", { role: "quality_inspector", modules: ["quality"], readyModules: ["quality"], permissions: ["quality.read"],
    scopeKind: "holding", capabilities: ["supply.purchasing", "supply.stock"] })).toBeNull();
  expect(productDestinationAvailable("people", "people-organization/people", { role: "payroll_preparer", modules: ["people_payroll"], readyModules: ["people_payroll"],
    permissions: ["payroll.read"], scopeKind: "holding", capabilities: ["people.directory", "people.lifecycle"] })).toBe(false);
});
it("does not synthesize role permissions when the private authority projection is absent", async () => {
  const f = fixture(); f.moduleAccess.mockResolvedValue({ modulesByAssignment: new Map([["role", ["finance"]]]),
    presentationModulesByAssignment: new Map([["role", ["finance"]]]), permissionsByAssignment: new Map(), beta: false });
  expect(await f.launch()).toEqual([]);
});


it('routes reports-only Front roles to their actual backoffice through normal Company selection while hiding roles with no current product permission',async()=>{
 for(const app of ['pos','pms'] as const){
  const f=fixture();f.grants.mockImplementation(async access=>[grant(access.scope.companyId,app)]);f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',[app]]]),presentationModulesByAssignment:new Map([['role',[app]]]),permissionsByAssignment:new Map([['role',[app+'.reports']]]),beta:false});
  const links=await f.launch();expect(links.length).toBeGreaterThan(0);for(const link of links){const sso=new URL(link.href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('companyId')).toBe(link.companyId);expect(launch.searchParams.get('next')).toBe(workspaceBase(link.scope)+'/'+app+'/reports');}
  f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',[app]]]),presentationModulesByAssignment:new Map([['role',[app]]]),permissionsByAssignment:new Map([['role',[]]]),beta:false});expect(await f.launch()).toEqual([]);
 }
});


it('keeps the Hotel minibar backoffice behind its existing add-on instead of treating it as a base Front landing',()=>{
 const context={role:'holding_owner',scopeKind:'holding' as const,modules:['pms'],readyModules:['pms'],capabilities:['pms.front-desk','pms.rooms']};expect(productDestinationAvailable('pms','pms/minibar',context)).toBe(false);expect(productDestinationAvailable('pms','pms/minibar',{...context,capabilities:[...context.capabilities,'pms.minibar','pms.linen']})).toBe(true);
});

it('enumerates Company Admin foundation across all authorized Companies and inherited locations without purchases or role selection',async()=>{
 const {createAdminLauncher}=await import('@erpx/server-work_projection/application/admin-launcher'),available=vi.fn(async()=>true),links=await createAdminLauncher({choices:async()=>choices(),available,origin:()=> 'https://admin-dev-erpx.jesadakorn.com',denied:()=>false})();expect(links).toHaveLength(4);expect(links.map(link=>`${link.companyId}/${link.workspace}`)).toEqual(['c1/Company One','c1/Branch One','c1/Branch One / Department One','c2/Branch Two / Department Two']);expect(available).toHaveBeenCalledTimes(4);const entry=new URL(new URL(links[3].href).searchParams.get('next')!,'https://admin-dev-erpx.jesadakorn.com');expect(entry.pathname).toBe('/admin/launch');expect(entry.searchParams.get('companyId')).toBe('c2');expect(entry.searchParams.get('next')).toBe('/holdings/acme/branches/branch-two/departments/department-two/administration');expect(JSON.stringify(links)).not.toMatch(/assignment|permissions|module|purchase|session/);
});
it('hides denied Company Admin and ERPX management scopes while surfacing unexpected source failures',async()=>{
 const {createAdminLauncher}=await import('@erpx/server-work_projection/application/admin-launcher'),erpx=buildWorkspaceHoldingChoice('erpx','ERPX',[{companyId:'provider-company',companyName:'ProviderCompany',scopeKind:'holding',scopeNodeId:null}],[],true)!,available=vi.fn(async input=>input.kind==='company');const ports={choices:async()=>[...choices(),erpx],available,origin:()=> 'https://admin-dev-erpx.jesadakorn.com',denied:(error:unknown)=>error instanceof Error&&error.message==='FORBIDDEN'};expect((await createAdminLauncher(ports)()).filter(link=>link.kind==='management')).toEqual([]);available.mockImplementation(async()=>{throw Error('FORBIDDEN');});expect(await createAdminLauncher(ports)()).toEqual([]);available.mockImplementation(async()=>{throw Error('SOURCE_FAILURE');});await expect(createAdminLauncher(ports)()).rejects.toThrow('SOURCE_FAILURE');
});


it('adds only the same current Company and location Admin foundation to the actual Office product picker',async()=>{
 const {OfficeProductMenus}=await import('@erpx/workplace/components/office-product-menus'),scope={kind:'holding' as const,holdingSlug:'acme'},base='/holdings/acme',link=(companyId:string,target:string)=>'https://admin-dev-erpx.jesadakorn.com/sso/start?'+new URLSearchParams({next:'/admin/launch?'+new URLSearchParams({companyId,next:target})});
 menuSources.products.mockResolvedValue([{holding:'Acme',company:'Current',companyId:'c1',workspace:'Current',scope,productId:'assets',href:'https://assets-dev-erpx.jesadakorn.com/sso/start?next=/',purchasePurpose:'new-work'},{holding:'Acme',company:'Other',companyId:'c2',workspace:'Other',scope,productId:'assets',href:'https://assets-dev-erpx.jesadakorn.com/sso/start?next=/other',purchasePurpose:'history'}]);
 menuSources.admin.mockResolvedValue([{holding:'Acme',company:'Current',companyId:'c1',workspace:'Current',kind:'company',href:link('c1',base+'/administration')},{holding:'Acme',company:'Other',companyId:'c2',workspace:'Other',kind:'company',href:link('c2',base+'/administration')},{holding:'Acme',company:'Current',companyId:'c1',workspace:'Branch',kind:'company',href:link('c1',base+'/branches/b1/administration')},{holding:'ERPX',company:'Current',companyId:'c1',workspace:'Current',kind:'management',href:link('c1','/holdings/erpx/erpx-management')}]);
 const view=await OfficeProductMenus({productId:'assets',companyId:'c1',scopeBase:base,workspaceLabel:'Current',language:'en'});expect(view.props.products).toEqual([{href:'https://assets-dev-erpx.jesadakorn.com/sso/start?next=/',label:'Assets & Maintenance',detail:undefined,current:true},{href:link('c1',base+'/administration'),label:'Company Admin'}]);expect(view.props.workspaces).toHaveLength(2);expect(JSON.stringify(view.props)).not.toMatch(/permissions|assignment|session|grants/);menuSources.admin.mockResolvedValue([]);expect((await OfficeProductMenus({productId:'assets',companyId:'c1',scopeBase:base,workspaceLabel:'Current',language:'th'})).props.products).toHaveLength(1);
});

it('discovers only the owner-proven included Stock entry for an existing Inventory role and keeps Department and read-only roles out',async()=>{
 const f=fixture();f.grants.mockImplementation(async access=>[grant(access.scope.companyId,'sales')]);f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',['inventory']]]),presentationModulesByAssignment:new Map([['role',['inventory']]]),permissionsByAssignment:new Map([['role',['inventory.read','inventory.prepare']]]),beta:false});
 const entry=vi.fn(async()=>null as {capability:string;purchasePurpose:'history'}|null),ports={...f,initialPhysicalStockCountEntry:entry};expect(await createProductLauncher(ports)()).toEqual([]);
 entry.mockResolvedValue({capability:'inventory.stock-core',purchasePurpose:'history'});const links=await createProductLauncher(ports)();expect(links).toHaveLength(2);expect(links.every(link=>link.productId==='sales'&&link.scope.kind!=='department'&&link.purchasePurpose==='history'&&link.sourceRecord===true)).toBe(true);expect(JSON.stringify(links)).not.toMatch(/assignmentId|inventory\.read|source_proof|contextKey/);
 for(const link of links){const launch=new URL(new URL(link.href).searchParams.get('next')!,'https://sales-dev-erpx.jesadakorn.com');expect(launch.searchParams.get('next')).toBe(`${workspaceBase(link.scope)}/${(await import('@erpx/server-commercial/public/rules/product-registry')).product('sales')!.landing}`);}
 entry.mockClear();f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',['inventory']]]),presentationModulesByAssignment:new Map([['role',['inventory']]]),permissionsByAssignment:new Map([['role',['inventory.read']]]),beta:false});expect(await createProductLauncher(ports)()).toEqual([]);expect(entry).not.toHaveBeenCalled();
 f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',['inventory']]]),presentationModulesByAssignment:new Map([['role',['inventory']]]),permissionsByAssignment:new Map([['role',['inventory.read','inventory.prepare']]]),beta:false});expect(await createProductLauncher({...ports,ready:async(_access,moduleId)=>moduleId!=='finance'})()).toEqual([]);
});


it('offers only the original Company Finance Manufacturing setup through normal selection and no module or authority synthesis',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'manufacturing')]);const originalAccess=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await originalAccess(scope,options);account.assignments[0].version=1;account.assignments[0].modules=[];return account;});
 let permissions=['finance.read','finance.write'],beta=false;const modules:string[]=[];
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,manufacturingPostingSetupEntry:owner};expect(await createProductLauncher(ports)()).toEqual([]);
 owner.mockResolvedValue({revision:'private-mfg-owner-image',purchasePurpose:'history'});
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({companyId:'c1',productId:'manufacturing',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/production/manufacturing');expect(JSON.stringify(links)).not.toMatch(/private-mfg|assignment|permissions|session|revision/);}
 beta=true;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();beta=false;
 for(const permission of [[],['finance.read'],['finance.approve'],['records.read']]){permissions=permission;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.write'];for(const release of ['manufacturing','inventory','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_access,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});

it('does not bypass the Manufacturing owner proof when registry discovery already chooses its Finance setup destination',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'manufacturing')]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0].version=1;return account;});
 f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',['manufacturing']]]),presentationModulesByAssignment:new Map([['role',['manufacturing']]]),permissionsByAssignment:new Map([['role',['finance.read','finance.write']]]),beta:false});const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,manufacturingPostingSetupEntry:owner};
 expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).toHaveBeenCalledTimes(1);owner.mockResolvedValue({revision:'retained-source',purchasePurpose:'history'});expect(await createProductLauncher(ports)()).toMatchObject([{productId:'manufacturing',purchasePurpose:'history',scope:{kind:'holding'}}]);
 for(const release of ['inventory','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_access,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});


it('discovers Manufacturing physical material counts for the existing Stock assignment without Department or generic Production authority',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'manufacturing')]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0].version=1;account.assignments[0].modules=['inventory'];return account;});
 let modules=['inventory'],permissions=['inventory.read','inventory.prepare'];f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta:false}));const owner=vi.fn(async()=>null as {capability:string;purchasePurpose:'history'}|null),ports={...f,manufacturingInitialPhysicalStockCountEntry:owner};
 expect(await createProductLauncher(ports)()).toEqual([]);owner.mockResolvedValue({capability:'inventory.stock-core',purchasePurpose:'history'});
 for(const current of [['inventory'],['manufacturing']]){modules=current;const links=await createProductLauncher(ports)();expect(links).toHaveLength(2);expect(links.every(link=>link.productId==='manufacturing'&&link.scope.kind!=='department'&&link.purchasePurpose==='history')).toBe(true);for(const link of links){const sso=new URL(link.href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe(workspaceBase(link.scope)+'/production/manufacturing');}expect(JSON.stringify(links)).not.toMatch(/inventory\.prepare|assignmentId|contextKey|private-proof/);}
 permissions=['inventory.read'];owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();permissions=['inventory.read','inventory.prepare'];for(const id of ['manufacturing','inventory','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_access,moduleId)=>moduleId!==id})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});

it('offers only the original Company Finance POS setup through normal selection and no module or authority synthesis',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'pos')]);const originalAccess=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await originalAccess(scope,options);account.assignments[0].version=1;account.assignments[0].modules=[];return account;});
 let permissions=['finance.read','finance.write'],beta=false;const modules:string[]=[];
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,posStartupEntry:owner};expect(await createProductLauncher(ports)()).toEqual([]);
 owner.mockResolvedValue({revision:'private-pos-owner-image',purchasePurpose:'history'});
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({companyId:'c1',productId:'pos',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/pos/settings/startup');expect(JSON.stringify(links)).not.toMatch(/private-pos|assignment|permissions|session|revision/);}
 beta=true;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();beta=false;
 for(const permission of [[],['finance.read'],['finance.approve'],['records.read']]){permissions=permission;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.write'];for(const release of ['pos','inventory','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_access,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});


it('offers POS Stock setup only for the real Company and Branch Stock role without creating POS discovery',async()=>{
 const f=fixture(),original=f.access.getMockImplementation()!;
 f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],role:'warehouse_manager',version:1,modules:[]};return account;});
 f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'pos')]);
 f.moduleAccess.mockResolvedValue({modulesByAssignment:new Map([['role',[]]]),presentationModulesByAssignment:new Map([['role',[]]]),permissionsByAssignment:new Map([['role',['inventory.read','inventory.prepare']]]),beta:false});
 const owner=vi.fn(async()=>({revision:'private-current-stock-proof',purchasePurpose:'history' as const})),links=await createProductLauncher({...f,posStartupEntry:owner})();
 expect(links.map(link=>link.scope.kind)).toEqual(['holding','branch']);
 for(const link of links){const sso=new URL(link.href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.searchParams.get('next')).toBe(`${workspaceBase(link.scope)}/pos/settings/startup`);}
 expect(JSON.stringify(links)).not.toMatch(/private-current|revision|assignment|permissions/);
});


it('discovers Assets accounting through the existing Company Finance role only after its owner proof',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'assets')]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0].version=1;account.assignments[0].modules=[];return account;});
 let permissions=['finance.read','finance.write'],beta=false;
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',[]]]),presentationModulesByAssignment:new Map([['role',[]]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,assetsFinancialEntry:owner};expect(await createProductLauncher(ports)()).toEqual([]);
 owner.mockResolvedValue({revision:'private-assets-image',purchasePurpose:'history'});
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({companyId:'c1',productId:'assets',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/services/assets/register');expect(JSON.stringify(links)).not.toMatch(/private-assets|assignment|permissions|session|revision/);}
 permissions=['finance.read'];expect(await createProductLauncher(ports)()).toMatchObject([{productId:'assets',purchasePurpose:'history'}]);
 beta=true;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();beta=false;
 for(const denied of [[],['finance.approve'],['records.read']]){permissions=denied;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.write'];for(const release of ['assets','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_account,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});


it('discovers Service parts accounting through the existing Company Finance role only after its owner proof',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'service'),grant(account.scope.companyId,'service-parts-assets')]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0].version=1;account.assignments[0].modules=[];return account;});
 let permissions=['finance.read','finance.write'],beta=false;
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',[]]]),presentationModulesByAssignment:new Map([['role',[]]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,servicePartsSetupEntry:owner};expect(await createProductLauncher(ports)()).toEqual([]);
 owner.mockResolvedValue({revision:'private-service-parts-image',purchasePurpose:'history'});
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({companyId:'c1',productId:'service',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/services/field-service');expect(JSON.stringify(links)).not.toMatch(/private-service|assignment|permissions|session|revision/);}
 permissions=['finance.read'];owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();
 beta=true;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();beta=false;
 for(const denied of [[],['finance.approve'],['records.read']]){permissions=denied;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.write'];for(const release of ['field_service','inventory','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_account,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});

it('discovers Projects worker preparation only after the real owner decision without changing the Company HR role or ordinary Projects landing',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'projects')]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0].version=1;account.assignments[0].modules=[];return account;});
 let permissions=['people.manage','organization.manage'],beta=false,modules:string[]=[];
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,projectWorkerStartupEntry:owner};expect(await createProductLauncher(ports)()).toEqual([]);owner.mockResolvedValue({revision:'private-project-worker-image',purchasePurpose:'history'});
 const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({companyId:'c1',productId:'projects',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/projects-routines/projects');expect(JSON.stringify(links)).not.toMatch(/private-project|assignment|permissions|session|revision/);
 for(const denied of [[],['people.manage'],['organization.manage'],['records.read']]){permissions=denied;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 permissions=['people.manage','organization.manage'];beta=true;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();beta=false;
 for(const denied of ['projects','people_payroll']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_account,id)=>id!==denied})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 modules=['projects'];permissions=['records.read','people.manage','organization.manage'];owner.mockClear();const ordinary=await createProductLauncher(ports)();expect(ordinary).toHaveLength(4);expect(ordinary.every(link=>link.productId==='projects'&&link.purchasePurpose==='new-work')).toBe(true);expect(owner).not.toHaveBeenCalled();
});

it('discovers Learning worker preparation only after the real owner decision without changing the Company HR role or ordinary Learning landing',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>[grant(account.scope.companyId,'learn')]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0].version=1;account.assignments[0].modules=[];return account;});
 let permissions=['people.manage','organization.manage'],beta=false,modules:string[]=[];
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>null as {revision:string;purchasePurpose:'history'}|null),ports={...f,learningWorkerStartupEntry:owner};expect(await createProductLauncher(ports)()).toEqual([]);owner.mockResolvedValue({revision:'private-learning-worker-image',purchasePurpose:'history'});
 const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({companyId:'c1',productId:'learn',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/knowledge/learning');expect(JSON.stringify(links)).not.toMatch(/private-learning|assignment|permissions|session|revision/);
 for(const denied of [[],['people.manage'],['organization.manage'],['records.read']]){permissions=denied;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 permissions=['people.manage','organization.manage'];beta=true;owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();beta=false;
 for(const denied of ['learning','people_payroll']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_account,id)=>id!==denied})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 modules=['learning'];permissions=['records.read','people.manage','organization.manage'];owner.mockClear();const ordinary=await createProductLauncher(ports)();expect(ordinary).toHaveLength(4);expect(ordinary.every(link=>link.productId==='learn'&&link.purchasePurpose==='new-work')).toBe(true);expect(owner).not.toHaveBeenCalled();
});


it('offers only exact included financial daily collections from native proof without People or base Finance discovery',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>['people','people-expenses','people-payroll'].map(id=>grant(account.scope.companyId,id)));
 const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],role:'custom_financial_settlement',version:1,modules:['finance']};return account;});
 let permissions=['module.finance.read','expense.pay','payroll.payment.read','finance.pay'],modules=['finance'],beta=false;
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>({revision:'private-financial-source',purchasePurpose:'history' as const})),ports={...f,peopleFinancialEntry:owner};
 const links=await createProductLauncher(ports)();expect(links).toHaveLength(2);
 expect(links.map(link=>[link.productId,link.scope.kind,link.purchasePurpose])).toEqual([['finance','holding','history'],['people','holding','history']]);
 for(const link of links){const sso=new URL(link.href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('next')).toBe('/holdings/acme/'+(link.productId==='finance'?'finance/documents/expenses':'people-organization/people/payroll/dashboard'));}
 expect(JSON.stringify(links)).not.toMatch(/private-financial|permissions|assignment|revision/);
 for(const change of [()=>{modules=[];},()=>{permissions=[];},()=>{beta=true;}]){change();owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();modules=['finance'];permissions=['module.finance.read','expense.pay','payroll.payment.read','finance.pay'];beta=false;}
 for(const release of ['people_payroll','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_account,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 owner.mockResolvedValue(null!);expect(await createProductLauncher(ports)()).toEqual([]);
});

// Owner-port integration only; the native Finance source is independently checked.
it('routes the Payroll posting prerequisite tile to its exact owner page without selecting a different role',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>['people','people-payroll'].map(id=>grant(account.scope.companyId,id)));const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],version:1,role:'custom_finance_approver',modules:['finance']};return account;});
 f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',['finance']]]),presentationModulesByAssignment:new Map([['role',['finance']]]),permissionsByAssignment:new Map([['role',['finance.approve']]]),beta:false}));
 const owner=vi.fn(async(account:AccountAccessContext,assignment:AccountAccessContext['assignments'][number])=>{void account;void assignment;return {revision:'private-prerequisite-source',purchasePurpose:'history' as const};}),ports={...f,payrollPostingPrerequisitesEntry:owner},links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({productId:'people',companyId:'c1',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('companyId')).toBe('c1');expect(launch.searchParams.get('next')).toBe('/holdings/acme/people-organization/people/payroll/settings/posting');expect(JSON.stringify(links)).not.toMatch(/private-prerequisite|assignment|permissions|session|revision/);expect(owner).toHaveBeenCalledTimes(1);expect(owner.mock.calls[0][1]).toMatchObject({role:'custom_finance_approver',modules:['finance'],version:1});
});
it('keeps Payroll prerequisite Apps discovery conditional on its Company Finance owner and both releases in active and retained history',async()=>{
 const f=fixture();let modules=['finance'],permissions=['finance.approve'],beta=false,eligible=true,history=false,version=1;
 f.grants.mockImplementation(async account=>['people','people-payroll'].map(id=>({...grant(account.scope.companyId,id),startsAt:history?2000:0,version:history?3:1})));f.history.mockImplementation(async account=>history?['people','people-payroll'].map(packageId=>({companyId:account.scope.companyId,packageId,packageVersion:1,sourceGrantVersion:1,sourceEventId:'private-old-'+packageId,retainedAt:0})):[]);
 const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],role:'custom_finance_approver',version,modules:['finance']};return account;});f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));
 const owner=vi.fn(async()=>eligible?{revision:'private-payroll-prerequisite',purchasePurpose:'history' as const}:null),ports={...f,payrollPostingPrerequisitesEntry:owner};
 for(const retained of [false,true]){history=retained;owner.mockClear();const links=await createProductLauncher(ports)();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({productId:'people',purchasePurpose:'history',scope:{kind:'holding'}});expect(owner).toHaveBeenCalledTimes(1);}eligible=false;expect(await createProductLauncher(ports)()).toEqual([]);eligible=true;
 for(const change of [()=>{modules=[];},()=>{modules=['people_payroll'];},()=>{permissions=['finance.read','finance.write'];},()=>{beta=true;},()=>{version=0;}]){change();owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();modules=['finance'];permissions=['finance.approve'];beta=false;version=1;}
 for(const release of ['people_payroll','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_actor,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
});

it('routes the Expense-only Finance setup tile to the exact source landing without selecting or synthesizing a People role',async()=>{
 const f=fixture();f.grants.mockImplementation(async account=>['people','people-expenses'].map(id=>grant(account.scope.companyId,id)));const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],version:1,role:'custom_expense_finance',modules:['finance']};return account;});f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',['finance']]]),presentationModulesByAssignment:new Map([['role',['finance']]]),permissionsByAssignment:new Map([['role',['finance.read','finance.write']]]),beta:false}));const owner=vi.fn(async(account:AccountAccessContext,assignment:AccountAccessContext['assignments'][number])=>{void account;void assignment;return {revision:'private-expense-setup-source',purchasePurpose:'history' as const};}),links=await createProductLauncher({...f,expensePostingPrerequisitesEntry:owner})();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({productId:'finance',companyId:'c1',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.pathname).toBe('/launch');expect(launch.searchParams.get('companyId')).toBe('c1');expect(launch.searchParams.get('next')).toBe('/holdings/acme/finance/setup/expenses');expect(JSON.stringify(links)).not.toMatch(/private-expense|assignment|permissions|session|revision/);expect(owner).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({scope:expect.objectContaining({kind:'holding',companyId:'c1'})}),expect.objectContaining({role:'custom_expense_finance',modules:['finance'],version:1}));
});
it('keeps Expense setup Apps fallback behind the real owner and both releases for active and retained histories without replacing an ordinary Finance landing',async()=>{
 const f=fixture();let modules=['finance'],permissions=['finance.read','finance.approve'],beta=false,eligible=true,history=false,version=1,fullFinance=false;f.grants.mockImplementation(async account=>[...['people','people-expenses'].map(id=>({...grant(account.scope.companyId,id),startsAt:history?2000:0,version:history?3:1})),...(fullFinance?[grant(account.scope.companyId,'finance')]:[])]);f.history.mockImplementation(async account=>history?['people','people-expenses'].map(packageId=>({companyId:account.scope.companyId,packageId,packageVersion:1,sourceGrantVersion:1,sourceEventId:'private-old-'+packageId,retainedAt:0})):[]);const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],role:'custom_expense_finance',version,modules:['finance']};return account;});f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',modules]]),presentationModulesByAssignment:new Map([['role',modules]]),permissionsByAssignment:new Map([['role',permissions]]),beta}));const owner=vi.fn(async()=>eligible?{revision:'private-expense-setup',purchasePurpose:'history' as const}:null),ports={...f,expensePostingPrerequisitesEntry:owner};for(const retained of [false,true]){history=retained;owner.mockClear();expect(await createProductLauncher(ports)()).toMatchObject([{productId:'finance',purchasePurpose:'history',scope:{kind:'holding'}}]);expect(owner).toHaveBeenCalledTimes(1);}eligible=false;expect(await createProductLauncher(ports)()).toEqual([]);eligible=true;
 for(const change of [()=>{modules=[];},()=>{modules=['people_payroll'];},()=>{permissions=['finance.read'];},()=>{permissions=['finance.approve'];},()=>{permissions=['finance.write'];},()=>{beta=true;},()=>{version=0;}]){change();owner.mockClear();expect(await createProductLauncher(ports)()).toEqual([]);expect(owner).not.toHaveBeenCalled();modules=['finance'];permissions=['finance.read','finance.approve'];beta=false;version=1;}for(const release of ['people_payroll','finance']){owner.mockClear();expect(await createProductLauncher({...ports,ready:async(_actor,id)=>id!==release})()).toEqual([]);expect(owner).not.toHaveBeenCalled();}
 history=false;fullFinance=true;permissions=builtInPermissions('holding_owner');owner.mockClear();const normal=await createProductLauncher(ports)();expect(normal.length).toBeGreaterThan(0);expect(normal.filter(link=>link.productId==='finance').every(link=>{const sso=new URL(link.href),launch=new URL(sso.searchParams.get('next')!,sso.origin);return launch.searchParams.get('next')?.endsWith('/finance/documents');})).toBe(true);expect(owner).not.toHaveBeenCalled();
});

it('offers the original Expense collection before setup for actual Finance accounting duties while leaving general Finance and People unavailable',async()=>{
 for(const action of ['expense.prepare','expense.approve']){const f=fixture();f.grants.mockImplementation(async account=>['people','people-expenses'].map(id=>grant(account.scope.companyId,id)));const original=f.access.getMockImplementation()!;f.access.mockImplementation(async(scope,options)=>{const account=await original(scope,options);account.assignments[0]={...account.assignments[0],role:'custom_expense_accounting',version:1,modules:['finance']};return account;});f.moduleAccess.mockImplementation(async()=>({modulesByAssignment:new Map([['role',['finance']]]),presentationModulesByAssignment:new Map([['role',['finance']]]),permissionsByAssignment:new Map([['role',['module.finance.read','finance.read','expense.read',action,action==='expense.prepare'?'finance.write':'finance.approve']]]),beta:false}));const owner=vi.fn(async()=>({revision:'private-original-accounting-source',purchasePurpose:'history' as const})),setup=vi.fn(async()=>({revision:'private-setup-source',purchasePurpose:'history' as const})),links=await createProductLauncher({...f,peopleFinancialEntry:owner,expensePostingPrerequisitesEntry:setup})();expect(links).toHaveLength(1);expect(links[0]).toMatchObject({productId:'finance',purchasePurpose:'history',scope:{kind:'holding'}});const sso=new URL(links[0].href),launch=new URL(sso.searchParams.get('next')!,sso.origin);expect(launch.searchParams.get('next')).toBe('/holdings/acme/finance/documents/expenses');expect(owner).toHaveBeenCalledExactlyOnceWith(expect.anything(),expect.objectContaining({role:'custom_expense_accounting',modules:['finance']}),{kind:'expense'});expect(setup).not.toHaveBeenCalled();expect(JSON.stringify(links)).not.toMatch(/private-original|private-setup|assignment|permission|revision/);}
});

it("limits a product host to the same authorized links as the complete launcher", async () => {
  const complete = fixture();
  complete.grants.mockImplementation(async access => [grant(access.scope.companyId, "finance"), grant(access.scope.companyId, "pos")]);
  const expected = (await createProductLauncher(complete)()).filter(choice => choice.productId === "finance");
  const scoped = fixture();
  scoped.grants.mockImplementation(async access => [grant(access.scope.companyId, "finance"), grant(access.scope.companyId, "pos")]);
  const actual = await createProductLauncher(scoped)("finance");
  expect(actual).toEqual(expected);
  expect(actual.length).toBeGreaterThan(0);
  expect(scoped.access.mock.calls).toEqual(complete.access.mock.calls);
  expect(await createProductLauncher(scoped)("learn")).toEqual([]);
});

it('uses the supplied Holding Front URL port for local POS choices', async () => {
  const f = fixture();
  f.grants.mockImplementation(async access => [grant(access.scope.companyId, 'pos')]);
  const front = vi.fn((target: string, app: 'pos'|'pms') => `https://acme.erpx.localhost:3443/${app}?source=${encodeURIComponent(target)}`);
  const links = await createProductLauncher({...f, holdingAddresses: true, holdingFrontUrl: front, origin: () => 'https://acme.erpx.localhost:3443'})('pos');
  expect(links.length).toBeGreaterThan(0);
  expect(front).toHaveBeenCalled();
  expect(links.every(link => new URL(link.href).origin === 'https://acme.erpx.localhost:3443')).toBe(true);
});
