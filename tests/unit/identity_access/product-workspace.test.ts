import {socialPostSourceEntry} from '@erpx/server-collaboration/public/rules/product-entry';
import {GET as launchGet,POST as launchPost} from "@erpx/workplace/routes/launch/route";
import {productFrontLaunch} from "@erpx/server-identity_access/public/product-front-launch";
import {financeDocumentEntry,salesCustomerReceiptEntry} from "@erpx/server-finance/public/rules/document-entry";
import {qualitySourceEntry} from "@erpx/server-quality/public/rules/product-entry";
import { beforeEach,afterEach, expect, it, vi } from "vitest";
import type { AccountAccessContext } from "@erpx/server-identity_access/public/authorization";
vi.mock("server-only", () => ({}));
const mocks = vi.hoisted(() => ({ access: vi.fn(), modules: vi.fn(), grants: vi.fn(), history: vi.fn(), ready: vi.fn(), selection: vi.fn(), select: vi.fn(),front:vi.fn(),legal:vi.fn(),posting:vi.fn(),salesPosting:vi.fn(),salesReceipt:vi.fn() }));
vi.mock("@erpx/server-finance/public/sales-receipt-sources",async importOriginal=>({...await importOriginal<typeof import("@erpx/server-finance/public/sales-receipt-sources")>(),salesCustomerReceiptProductEntry:mocks.salesReceipt}));
vi.mock("@erpx/server-finance/public/sales-posting-setup",()=>({salesPostingSetupProductEntry:mocks.salesPosting}));
vi.mock("@erpx/server-commerce/public/posting-setup",()=>({commercePostingSetupProductEntry:mocks.posting}));
vi.mock("@erpx/server-finance/public/legal-setup",()=>({financeProductLegalSetupEntry:mocks.legal}));
vi.mock("@erpx/server-quality/public/product-entry",()=>({qualityProductSourceEntry:vi.fn()}));
vi.mock("@erpx/server-people_payroll/public/self-service-product-entry",()=>({workforceProductSelfServiceEntry:vi.fn()}));
vi.mock("@erpx/server-collaboration/public/product-entry",()=>({socialPostProductSourceEntry:vi.fn()}));
vi.mock("@erpx/server-projects/public/product-entry",()=>({projectTaskProductSourceEntry:vi.fn()}));
vi.mock("@erpx/server-talent/public/product-entry",()=>({talentHireProductSourceEntry:vi.fn(),talentCandidatePrivacyProductSourceEntry:vi.fn()}));
vi.mock("@erpx/server-identity_access/adapters/authorization", () => ({ requireAccountAccess: mocks.access }));
vi.mock("@erpx/server-identity_access/adapters/work-module-access", () => ({ accountWorkModuleAccess: mocks.modules }));
vi.mock("@erpx/server-commercial/public/package-grants", () => ({ readCompanyPurchaseFacts: async (...args: unknown[]) => ({grants:await mocks.grants(...args),history:await mocks.history(...args)}) }));
vi.mock("@erpx/server-runtime/public/runtime", () => ({ holdingResources: async () => ({ write: {} }) }));
vi.mock("@erpx/server-pms/public/feature-availability", () => ({ assertProductionModuleEnabled: mocks.ready }));
vi.mock("@erpx/server-work_projection/public/work-role-store", () => ({ initializeWorkSelection: mocks.selection, selectWorkRole: mocks.select }));
vi.mock("@erpx/server-organization/public/product-company", () => ({ productCompany: async () => ({ id: "c2", name: "Fixture Company" }) }));
vi.mock("@erpx/server-work_projection/public/front-app",()=>({frontCompanyApp:mocks.front}));
vi.mock("@erpx/feature-runtime/lib/product-hosts", () => ({ productHostForHost: (host: string) => host === "finance-dev-erpx.jesadakorn.com" ? "finance" : host==="pms-dev-erpx.jesadakorn.com"?"pms":host==="pos-dev-erpx.jesadakorn.com"?"pos":host==="supply-dev-erpx.jesadakorn.com"?"supply":host==="sales-dev-erpx.jesadakorn.com"?"sales":null,
  configuredProductOrigin: (key:string) => `https://${key}-dev-erpx.jesadakorn.com` }));
import { productWorkspaceRequest } from "@erpx/server-identity_access/adapters/product-workspace";
import { ApiError } from "@erpx/server-runtime/public/http";
import { createProductWorkspace } from "@erpx/server-identity_access/application/product-workspace";
import { productWorkspaceTarget } from "@erpx/server-identity_access/domain/product-workspace";
import { builtInPermissions } from "@erpx/feature-identity_access/lib/permissions";

const request = () => new Request("https://finance-dev-erpx.jesadakorn.com/launch?companyId=c2&next=%2Fholdings%2Facme%2Ffinance%2Fdocuments%3Fstatus%3Dpaid", { headers: { host: "finance-dev-erpx.jesadakorn.com" } });
const access = (): AccountAccessContext => ({ scope: { kind: "holding", holdingSlug: "acme", holdingId: "h001", holdingName: "Acme", companyId: "c2" },
  sessionId: "session", expiresAt: Date.now() + 1000, userId: "user", name: "User", email: "fixture@example.test", requestHeaders: new Headers(),
  assignments: [{ id: "assignment", role: "holding_owner", companyId: "c2", branchId: null, modules: ["finance"], scopeKind: "holding", scopeNodeId: null, version: 1 }] });
const selected = () => ({ session_id: "session", user_id: "user", company_id: "c2", assignment_id: "assignment", assignment_version: 1, revision: 3, expires_at: Date.now() + 60_000 });
it("launches Company Executive on Work with its current permission and independent release, without buying Finance or changing the role",async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=["executive.access"],released=true,visible=false;
 const ready=vi.fn(async(_access:unknown,id:string)=>id!=="executive"||released),executiveVisible=vi.fn(async()=>visible),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([["assignment",[]]]),presentationModulesByAssignment:new Map([["assignment",[]]]),permissionsByAssignment:new Map([["assignment",permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:"c2",packageId:"work",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Fixture",evidence:"test:original-executive-entry"}],history:[]}),ready,executiveVisible,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const target="/holdings/acme/insights/executive/reports/original?tag=one&tag=two#canvas",input=productWorkspaceTarget("work","c2",target)!;
 await expect(service.read("work",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});expect(executiveVisible).toHaveBeenCalledWith(account,account.assignments[0]);
 visible=true;expect(await service.read("work",input)).toMatchObject({target});expect(ready).toHaveBeenCalledWith(account,"executive");expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
 released=false;await expect(service.read("work",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});released=true;permissions=[];visible=false;await expect(service.read("work",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});
 // A current shared-report reader is visible even without general author permission.
 visible=true;expect(await service.read("work",input)).toMatchObject({target});
 expect(productWorkspaceTarget("finance","c2",target)).toBeNull();expect(productWorkspaceTarget("admin","c2",target)).toBeNull();
});
beforeEach(() => {
  // This suite exercises required Commercial admission regardless of installed dev flags.
  vi.stubEnv('ERPX_CONSOLE_RETIRED','disabled');
  for (const mock of Object.values(mocks)) mock.mockReset();
  mocks.access.mockResolvedValue(access());
  mocks.modules.mockResolvedValue({ modulesByAssignment: new Map([["assignment", ["finance"]]]),
    presentationModulesByAssignment: new Map([["assignment", ["finance"]]]), permissionsByAssignment: new Map([["assignment", builtInPermissions("holding_owner")]]), beta: false });
  mocks.grants.mockResolvedValue([{ companyId: "c2", packageId: "finance", packageVersion: 1, state: "active", startsAt: 0, endsAt: null, version: 1, reason: "Fixture", evidence: "test:fixture" }]);
  mocks.history.mockResolvedValue([]);
  mocks.selection.mockResolvedValue(selected());
});
afterEach(()=>vi.unstubAllEnvs());
it('preserves existing Company and role authority when development Commercial admission is deferred',async()=>{
 let released=true;
 const service=createProductWorkspace({access:async()=>access(),modules:mocks.modules,purchaseFacts:async()=>({grants:[],history:[]}),
  commercialAdmissionDeferred:id=>id==='finance',ready:async()=>released,selection:mocks.selection,select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('finance','c2','/holdings/acme/finance/documents?state=kept')!;
 expect(await service.read('finance',input)).toMatchObject({target:input.target});expect(mocks.select).not.toHaveBeenCalled();
 released=false;await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});released=true;
 mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',[]]]),beta:false});
 await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 expect(productWorkspaceTarget('people','c2',input.target)).toBeNull();
});
it("checks explicit Company access before setting a host-only cookie and preserves the target query", async () => {
  const response = await productWorkspaceRequest(request());
  expect(mocks.access).toHaveBeenCalledWith({ kind: "holding", holdingSlug: "acme" }, { companyId: "c2" });
  expect(response.status).toBe(303);
  expect(response.headers.get("location")).toBe("https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents?status=paid");
  const cookie = response.headers.get("set-cookie")!;
  for (const value of ["erpx_company_h001=c2", "Secure", "HttpOnly", "SameSite=lax"]) expect(cookie).toContain(value);
  expect(cookie).not.toMatch(/domain=/i);
  expect(response.headers.get("cache-control")).toBe("private, no-store");
});
it("allows the current role to enter an expired or draining purchase for existing work", async () => {
  for(const patch of [{state:"draining"},{endsAt:Date.now()-1}]){
    mocks.grants.mockResolvedValue([{companyId:"c2",packageId:"finance",packageVersion:1,state:"active",startsAt:0,endsAt:null,version:1,reason:"Fixture",evidence:"test:fixture",...patch}]);
    const response=await productWorkspaceRequest(request());
    expect(response.status).toBe(303);
    expect(response.headers.get('location')).toBe('https://finance-dev-erpx.jesadakorn.com/holdings/acme/finance/documents?status=paid');
  }
});
it("allows retained history entry while preserving current authority and keeping private facts off the response", async () => {
  mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'finance',packageVersion:1,state:'active',startsAt:Date.now()+60000,endsAt:null,version:4,reason:'Fixture',evidence:'test:fixture'}]);
  mocks.history.mockResolvedValue([{companyId:'c2',packageId:'finance',packageVersion:1,sourceGrantVersion:1,sourceEventId:'private-retained-event',retainedAt:0}]);
  const response=await productWorkspaceRequest(request());expect(response.status).toBe(303);expect(response.headers.get('location')).toContain('/finance/documents?status=paid');
  expect(await response.text()).not.toContain('private-retained-event');
  mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',[]]]),beta:false});
  expect((await productWorkspaceRequest(request())).status).toBe(403);
});
it("does not set a Company cookie for a first future purchase or foreign history", async () => {
  mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'finance',packageVersion:1,state:'active',startsAt:Date.now()+60000,endsAt:null,version:1,reason:'Fixture',evidence:'test:fixture'}]);
  mocks.history.mockResolvedValue([{companyId:'foreign',packageId:'finance',packageVersion:1,sourceGrantVersion:1,sourceEventId:'foreign-retained-event',retainedAt:0}]);
  const response=await productWorkspaceRequest(request());expect(response.status).toBe(403);expect(response.headers.get('set-cookie')).toBeNull();expect(mocks.selection).not.toHaveBeenCalled();
});
it("asks for a role without changing the live selection when another account role can enter", async () => {
  mocks.selection.mockResolvedValue({ ...selected(), assignment_id: "another-role" });
  const response = await productWorkspaceRequest(request());
  expect(response.status).toBe(303);
  const target = new URL(response.headers.get("location")!);
  expect(target.pathname).toBe("/choose-work-role");
  expect(target.searchParams.get("next")).toBe("/holdings/acme/finance/documents?status=paid");
  expect(mocks.select).not.toHaveBeenCalled();
});
it("rejects a missing purchase and unready product before initializing a role", async () => {
  mocks.grants.mockResolvedValue([]);
  expect((await productWorkspaceRequest(request())).status).toBe(403);
  expect(mocks.selection).not.toHaveBeenCalled();
  mocks.grants.mockResolvedValue([{ companyId: "c2", packageId: "finance", packageVersion: 1, state: "active", startsAt: 0, endsAt: null, version: 1, reason: "Fixture", evidence: "test:fixture" }]);
  mocks.ready.mockRejectedValue(new ApiError(403, "FEATURE_COMING_SOON", "Not released."));
  expect((await productWorkspaceRequest(request())).status).toBe(403);
  expect(mocks.selection).not.toHaveBeenCalled();
});
const post = (overrides: Record<string, string> = {}, origin = "https://finance-dev-erpx.jesadakorn.com", fetchMetadata: Record<string, string> = {}) => new Request("https://finance-dev-erpx.jesadakorn.com/launch", {
  method: "POST", headers: { host: "finance-dev-erpx.jesadakorn.com", origin, "content-type": "application/x-www-form-urlencoded", ...fetchMetadata },
  body: new URLSearchParams({ companyId: "c2", next: "/holdings/acme/finance/documents?status=paid", assignmentId: "assignment", assignmentVersion: "1", revision: "3", ...overrides }),
});
it("uses the normal versioned role service only for an explicit same-origin selection", async () => {
  mocks.selection.mockResolvedValue({ ...selected(), assignment_id: "another-role" });
  mocks.select.mockResolvedValue(selected());
  const response = await productWorkspaceRequest(post());
  expect(response.status).toBe(303);
  expect(new URL(response.headers.get("location")!).pathname).toBe("/launch");
  expect(mocks.select).toHaveBeenCalledWith({}, expect.objectContaining({ userId: "user", companyId: "c2", assignmentId: "assignment", assignmentVersion: 1, revision: 3 }));
  mocks.select.mockClear();
  expect((await productWorkspaceRequest(post({}, "https://attacker.invalid"))).status).toBe(403);
  expect(mocks.select).not.toHaveBeenCalled();
});
it("accepts opaque Origin only for a native same-origin document role navigation", async () => {
  mocks.selection.mockResolvedValue({ ...selected(), assignment_id: "another-role" });
  mocks.select.mockResolvedValue(selected());
  const response = await productWorkspaceRequest(post({}, "null", {
    "sec-fetch-site": "same-origin", "sec-fetch-mode": "navigate", "sec-fetch-dest": "document",
  }));
  expect(response.status).toBe(303);
  expect(mocks.select).toHaveBeenCalledWith({}, expect.objectContaining({ userId: "user", companyId: "c2", assignmentId: "assignment", assignmentVersion: 1, revision: 3 }));
  mocks.select.mockClear();
  for (const metadata of [
    { "sec-fetch-site": "cross-site", "sec-fetch-mode": "navigate", "sec-fetch-dest": "document" },
    { "sec-fetch-site": "same-origin", "sec-fetch-mode": "cors", "sec-fetch-dest": "document" },
    { "sec-fetch-site": "same-origin", "sec-fetch-mode": "navigate", "sec-fetch-dest": "empty" },
  ]) expect((await productWorkspaceRequest(post({}, "null", metadata))).status).toBe(403);
  expect((await productWorkspaceRequest(post({}, "null"))).status).toBe(403);
  expect((await productWorkspaceRequest(post({}, "https://attacker.invalid", {
    "sec-fetch-site": "same-origin", "sec-fetch-mode": "navigate", "sec-fetch-dest": "document",
  }))).status).toBe(403);
  expect(mocks.select).not.toHaveBeenCalled();
});
it("recovers stale context and unavailable role choices without selecting another role", async () => {
  for (const fields of [{ revision: "2" }, { assignmentId: "foreign-role" }, { assignmentVersion: "2" }] as Record<string, string>[]) {
    const response = await productWorkspaceRequest(post(fields));
    expect(response.status).toBe(303);
    const url = new URL(response.headers.get("location")!);
    expect(url.pathname).toBe("/choose-work-role"); expect(url.searchParams.get("notice")).toBe("access-changed");
    expect(mocks.select).not.toHaveBeenCalled();
  }
});
it("chooses a narrow role's owner landing and preserves the query and anchor without changing its Module IDs", async () => {
  const account = access(); account.assignments[0] = { ...account.assignments[0], role: "custom_marketing", modules: ["marketing"] };
  const authority = { modulesByAssignment: new Map([["assignment", ["marketing"]]]), presentationModulesByAssignment: new Map([["assignment", ["marketing"]]]),
    permissionsByAssignment: new Map([["assignment", ["records.read"]]]), beta: false };
  const service = createProductWorkspace({ access: async () => account, modules: async () => authority,
    purchaseFacts: async () => ({grants:["sales", "sales-marketing"].map(packageId => ({ companyId: "c2", packageId, packageVersion: 1, state: "active" as const, startsAt: 0, endsAt: null, version: 1, reason: "Fixture", evidence: "test:fixture" })),history:[]}),
    ready: async () => true, selection: async () => selected(), select: mocks.select, now: Date.now,
    error: (status, code, message) => new ApiError(status, code, message) });
  const input = productWorkspaceTarget("sales", "c2", "/holdings/acme/customers-sales/sales/pipeline?q=launch#draft")!;
  expect((await service.read("sales", input)).target).toBe("/holdings/acme/customers-sales/marketing?q=launch#draft");
  expect(account.assignments[0].modules).toEqual(["marketing"]);
  expect(mocks.select).not.toHaveBeenCalled();
});
it("does not treat account inspection as an applicable work assignment", async () => {
  for (const assignments of [[], [{ ...access().assignments[0], companyId: "other" }], [{ ...access().assignments[0], scopeKind: "department", scopeNodeId: "d1" }]]) {
    mocks.access.mockResolvedValue({ ...access(), assignments });
    const response = await productWorkspaceRequest(request());
    expect(response.status).toBe(403); expect(response.headers.get("set-cookie")).toBeNull();
  }
});
it("retains directory/session tombstone failures without creating a Company cookie", async () => {
  for (const code of ["SESSION_REVOKED", "DIRECTORY_UNAVAILABLE"]) {
    mocks.access.mockRejectedValue(new ApiError(401, code, "Sign in again."));
    const response = await productWorkspaceRequest(request());
    expect(response.status).toBe(401); expect(response.headers.get("set-cookie")).toBeNull();
    expect(response.headers.get("cache-control")).toBe("private, no-store");
  }
});

it("restricts included Finance entry to exact canonical retained document IDs",()=>{
 expect(financeDocumentEntry(`finance/accounting/journals/${"long-".repeat(45)}/overview`)).toMatchObject({kind:"journal",id:"long-".repeat(45)});
 for(const [path,kind] of [["documents/invoices","invoice"],["accounting/journals","journal"],["cash/payments","payment"],["cash/refunds","refund"],["cash/supplier-batches","supplier-batch"]] as const){
  expect(financeDocumentEntry(`finance/${path}/real-id/overview`)).toEqual({kind,id:"real-id"});
  expect(financeDocumentEntry(`finance/${path}/real-id`)).toEqual({kind,id:"real-id"});
  for(const suffix of ["","/real-id/history","/../overview","/real-id/overview/extra"] )expect(financeDocumentEntry(`finance/${path}${suffix}`)).toBeNull();
 }
 for(const path of ["finance/setup","finance/accounting","finance/documents/invoices?source=pos","finance/cash/payments/real?id=other"])expect(financeDocumentEntry(path)).toBeNull();
});
it('accepts only exact existing Quality and Count source destinations for included entry',()=>{
 for(const [area,kind]of [['quality','hold'],['counts','count']]as const){
  expect(qualitySourceEntry(`inventory-quality/${area}/actual-source`)).toEqual({kind,id:'actual-source'});
  expect(qualitySourceEntry(`inventory-quality/${area}/actual-source/overview`)).toEqual({kind,id:'actual-source'});
  for(const suffix of ['', '/new','/actual-source/history','/actual-source/overview/extra','/../overview','/actual-source?source=sales','/actual-source#source'])expect(qualitySourceEntry(`inventory-quality/${area}${suffix}`)).toBeNull();
 }
});
it('offers an owner-proven source Quality or disposal Count through Supply without discovering its collections or automatically replacing a role',async()=>{
 const account=access();account.assignments[0].modules=['sales'];
 const authority={modulesByAssignment:new Map([['assignment',['sales']]]),presentationModulesByAssignment:new Map([['assignment',['sales']]]),permissionsByAssignment:new Map([['assignment',['quality.read','inventory.read']]]),beta:false};
 const source=vi.fn().mockResolvedValue({capability:'sales.orders',purchasePurpose:'history'}),ready=vi.fn().mockResolvedValue(true);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'sales',packageVersion:1,state:'draining',startsAt:0,endsAt:null,version:2,reason:'Fixture',evidence:'test:quality-source-entry'}],history:[]}),ready,qualitySourceEntry:source,selection:async()=>({...selected(),assignment_id:'another-role'}),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const [area,kind,owner]of [['quality','hold','quality'],['counts','count','inventory']]as const){
  const input=productWorkspaceTarget('supply','c2',`/holdings/acme/inventory-quality/${area}/actual-source/overview?tag=one&tag=two#history`)!;
  const state=await service.read('supply',input);expect(state.target).toBeNull();expect(state.options).toMatchObject([{assignmentId:'assignment',assignmentVersion:1,target:input.target,purchasePurpose:'history',sourceRecord:true}]);expect(source).toHaveBeenLastCalledWith(account,account.assignments[0],{kind,id:'actual-source'});expect(ready).toHaveBeenCalledWith(account,owner);
 }
 source.mockClear();for(const area of ['quality','counts'])await expect(service.read('supply',productWorkspaceTarget('supply','c2',`/holdings/acme/inventory-quality/${area}`)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 expect(source).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['sales']);
});
it('requires current owner permission, source and release for included Quality entry even alongside Supply base',async()=>{
 const account=access();account.assignments[0].modules=['quality'];
 const authority={modulesByAssignment:new Map([['assignment',['quality']]]),presentationModulesByAssignment:new Map([['assignment',['quality']]]),permissionsByAssignment:new Map([['assignment',['quality.read']]]),beta:false};
 let sourceAllowed=true,released=true;const source=vi.fn(async()=>sourceAllowed?{capability:'supply.receiving',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'supply',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:basic-quarantine-entry'}],history:[]}),ready:async()=>released,qualitySourceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('supply','c2','/holdings/acme/inventory-quality/quality/actual-source/overview')!;
 expect((await service.read('supply',input)).sourceRecord).toBe(true);sourceAllowed=false;await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 sourceAllowed=true;released=false;await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});released=true;
 authority.permissionsByAssignment.set('assignment',[]);await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(mocks.select).not.toHaveBeenCalled();
});
it('offers only an actual Project task as included Work entry while preserving Company, role and exact return queries',async()=>{
 const account=access();account.assignments[0].modules=['projects'];
 const authority={modulesByAssignment:new Map([['assignment',['projects']]]),presentationModulesByAssignment:new Map([['assignment',['projects']]]),permissionsByAssignment:new Map([['assignment',['records.read']]]),beta:false};
 const source=vi.fn().mockResolvedValue({capability:'projects.tasks',purchasePurpose:'history'}),ready=vi.fn().mockResolvedValue(true);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'projects',packageVersion:1,state:'draining',startsAt:0,endsAt:null,version:2,reason:'Fixture',evidence:'test:project-task-source'}],history:[]}),ready,projectTaskSourceEntry:source,selection:async()=>({...selected(),assignment_id:'another-role'}),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('work','c2','/holdings/acme/projects-routines/tasks/actual-project-task/overview?tag=one&tag=two#history')!,state=await service.read('work',input);
 expect(state.target).toBeNull();expect(state.options).toMatchObject([{assignmentId:'assignment',assignmentVersion:1,target:input.target,purchasePurpose:'history',sourceRecord:true}]);expect(source).toHaveBeenCalledWith(account,account.assignments[0],{kind:'task',id:'actual-project-task'});expect(ready).toHaveBeenCalledWith(account,'projects');expect(ready).toHaveBeenCalledWith(account,'operations');
 source.mockClear();for(const path of ['projects-routines/tasks','projects-routines/tasks/new','work','inbox/messages'])await expect(service.read('work',productWorkspaceTarget('work','c2',`/holdings/acme/${path}`)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 expect(source).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['projects']);
});
it('refuses included Project task entry after source, current permission or either owner release becomes unavailable',async()=>{
 const account=access(),authority={modulesByAssignment:new Map([['assignment',['projects']]]),presentationModulesByAssignment:new Map([['assignment',['projects']]]),permissionsByAssignment:new Map([['assignment',['records.read']]]),beta:false};
 let available=true,unreleased='';const source=vi.fn(async()=>available?{capability:'projects.tasks',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[],history:[]}),ready:async(_access,id)=>id!==unreleased,projectTaskSourceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('work','c2','/holdings/acme/projects-routines/tasks/actual-project-task')!;expect((await service.read('work',input)).sourceRecord).toBe(true);
 available=false;await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});available=true;
 for(const id of ['projects','operations']){unreleased=id;await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});}unreleased='';authority.permissionsByAssignment.set('assignment',[]);await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(mocks.select).not.toHaveBeenCalled();
});
it("uses actual owner source entry without discovering Finance or changing the live role",async()=>{
 const account=access(),facts={grants:[{companyId:"c2",packageId:"pos",packageVersion:1,state:"active" as const,startsAt:0,endsAt:null,version:1,reason:"Fixture",evidence:"test:source-entry"}],history:[]};
 account.assignments[0].modules=["pos"];
 const authority={modulesByAssignment:new Map([["assignment",["pos"]]]),presentationModulesByAssignment:new Map([["assignment",["pos"]]]),permissionsByAssignment:new Map([["assignment",["finance.read"]]]),beta:false};
 const source=vi.fn().mockResolvedValue({capability:"finance.pos-posting",purchasePurpose:"history"}),ready=vi.fn().mockResolvedValue(true);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>facts,ready,financeDocumentEntry:source,selection:async()=>({...selected(),assignment_id:"another-role"}),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget("finance","c2","/holdings/acme/finance/documents/invoices/actual-id/overview?returnTo=%2Fholdings%2Facme%2Fpos#history")!,state=await service.read("finance",input);
 expect(state.target).toBeNull();expect(state.options).toMatchObject([{assignmentId:"assignment",target:input.target,purchasePurpose:"history",sourceRecord:true}]);expect(ready).toHaveBeenCalledWith(account,"finance");expect(source).toHaveBeenCalledWith(account,account.assignments[0],{kind:"invoice",id:"actual-id"});expect(mocks.select).not.toHaveBeenCalled();
 await expect(service.read("finance",productWorkspaceTarget("finance","c2","/holdings/acme/finance/documents")!)).rejects.toMatchObject({code:"PRODUCT_PACKAGE_REQUIRED"});
 expect(account.assignments[0].modules).toEqual(["pos"]);
});
it("keeps included Finance record entry closed when source, current authority or release is unavailable",async()=>{
 const account=access(),authority={modulesByAssignment:new Map([["assignment",["finance"]]]),presentationModulesByAssignment:new Map([["assignment",["finance"]]]),permissionsByAssignment:new Map([["assignment",["finance.read"]]]),beta:false};
 let sourceAllowed=true,ready=true;const source=vi.fn(async()=>sourceAllowed?{capability:"finance.pos-posting",purchasePurpose:"history" as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[],history:[]}),ready:async()=>ready,financeDocumentEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget("finance","c2","/holdings/acme/finance/accounting/journals/actual-id/overview")!;
 expect((await service.read("finance",input)).sourceRecord).toBe(true);
 sourceAllowed=false;await expect(service.read("finance",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});sourceAllowed=true;ready=false;await expect(service.read("finance",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});ready=true;
 authority.permissionsByAssignment.set("assignment",[]);await expect(service.read("finance",input)).rejects.toMatchObject({code:"PRODUCT_ROLE_REQUIRED"});expect(mocks.select).not.toHaveBeenCalled();
});

it("launches a host-bound Front record through the original Company/role/location reader and preserves its query and fragment",async()=>{
 mocks.front.mockResolvedValue({companyId:'c2',locations:[{path:'branches/b1/departments/d1'}]});
 const target='/acme/acme-co/branches/b1/departments/d1/guest-accounts/records/pms_record/g1/overview?property=h1#charges',request=new Request('https://pms-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:target}),{headers:{host:'pms-dev-erpx.jesadakorn.com'}}),response=await productFrontLaunch(request);
 expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://pms-dev-erpx.jesadakorn.com'+target);expect(response.headers.get('cache-control')).toBe('private, no-store');expect(response.headers.get('referrer-policy')).toBe('no-referrer');expect(response.headers.get('set-cookie')).toBeNull();expect(mocks.front).toHaveBeenCalledWith('acme','acme-co','pms');expect(mocks.select).not.toHaveBeenCalled();
});
it("refuses Front Company/location/account changes and unavailable current roles before navigation",async()=>{
 const request=()=>new Request('https://pms-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:'/acme/acme-co/holding/guest-accounts'}),{headers:{host:'pms-dev-erpx.jesadakorn.com'}});
 for(const entry of [null,{companyId:'foreign',locations:[{path:'holding'}]},{companyId:'c2',locations:[]}]){mocks.front.mockResolvedValue(entry);const response=await productFrontLaunch(request());expect(response.status).toBe(403);expect(response.headers.get('location')).toBeNull();}
 mocks.front.mockRejectedValue(new ApiError(401,'SESSION_REVOKED','Sign in again.'));expect((await productFrontLaunch(request())).status).toBe(401);expect(mocks.select).not.toHaveBeenCalled();
});
it("keeps Front launch GET-only and rejects untrusted returns, duplicate targets and legacy hosts",async()=>{
 const url='https://pos-dev-erpx.jesadakorn.com/launch?companyId=c2&next=%2Facme%2Facme-co%2Fholding%2Fsale';const post=await productFrontLaunch(new Request(url,{method:'POST',headers:{host:'pos-dev-erpx.jesadakorn.com'}}));expect(post.status).toBe(405);expect(post.headers.get('allow')).toBe('GET');expect(post.headers.get('location')).toBeNull();
 for(const next of ['https://evil.test','/api/auth/sign-out','/acme/acme-co/holding/guest-accounts'])expect((await productFrontLaunch(new Request('https://pos-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next}),{headers:{host:'pos-dev-erpx.jesadakorn.com'}}))).status).toBe(400);
 expect((await productFrontLaunch(new Request(url+'&next=other',{headers:{host:'pos-dev-erpx.jesadakorn.com'}}))).status).toBe(422);expect((await productFrontLaunch(new Request(url,{headers:{host:'dev-pos.jesadakorn.com'}}))).status).toBe(400);expect(mocks.front).not.toHaveBeenCalled();
});

it("sends browser-only launch denials to the existing role/package recovery while JSON and mutation failures retain their status",async()=>{
 mocks.grants.mockResolvedValue([]);const original=request(),browser=new Request(original.url,{headers:{host:'finance-dev-erpx.jesadakorn.com',accept:'text/html'}}),response=await launchGet(browser);expect(response.status).toBe(303);const recovery=new URL(response.headers.get('location')!);expect(recovery.origin).toBe('https://finance-dev-erpx.jesadakorn.com');expect(recovery.pathname).toBe('/choose-work-role');expect(recovery.searchParams.get('companyId')).toBe('c2');expect(recovery.searchParams.get('next')).toBe('/holdings/acme/finance/documents?status=paid');expect(response.headers.get('set-cookie')).toBeNull();expect((await launchGet(original)).status).toBe(403);expect(mocks.select).not.toHaveBeenCalled();
});


it('launches Front backoffice through current Company purchase and normal versioned work-role selection without changing the native GET launch',async()=>{
 for(const app of ['pos','pms'] as const){
  const account=access();account.assignments[0].modules=[app];mocks.access.mockResolvedValue(account);mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['assignment',[app]]]),presentationModulesByAssignment:new Map([['assignment',[app]]]),permissionsByAssignment:new Map([['assignment',builtInPermissions('holding_owner')]]),beta:false});mocks.grants.mockResolvedValue([{companyId:'c2',packageId:app,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:front-backoffice'}]);
  const target='/holdings/acme/'+app+'/settings?property=h1&tag=one&tag=two#history',url='https://'+app+'-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:target}),response=await launchGet(new Request(url,{headers:{host:app+'-dev-erpx.jesadakorn.com'}}));
  expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://'+app+'-dev-erpx.jesadakorn.com'+target);expect(response.headers.get('set-cookie')).toContain('erpx_company_h001=c2');expect(mocks.front).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();
  mocks.selection.mockResolvedValue({...selected(),assignment_id:'other'});const choose=await productWorkspaceRequest(new Request(url,{headers:{host:app+'-dev-erpx.jesadakorn.com'}}));expect(new URL(choose.headers.get('location')!).pathname).toBe('/choose-work-role');expect(mocks.select).not.toHaveBeenCalled();
  mocks.selection.mockResolvedValue(selected());mocks.grants.mockResolvedValue([]);expect((await productWorkspaceRequest(new Request(url,{headers:{host:app+'-dev-erpx.jesadakorn.com'}}))).status).toBe(403);
 }
 for(const app of ['pos','pms'] as const){expect(productWorkspaceTarget(app,'c2','/holdings/acme/finance/setup')).toBeNull();expect(productWorkspaceTarget(app,'c2','/acme/acme-co/holding/')).toBeNull();expect(productWorkspaceTarget(app,'c2','/holdings/acme/'+app+'-private/settings')).toBeNull();}
});
it('uses only the exact legal setup foundation entry after current Finance authority and release, with no full Finance discovery',async()=>{
 mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'pos',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:legal-setup'}]);mocks.legal.mockResolvedValue({revision:'private-current-capability-revision'});
 const request=(suffix='thai-tax')=>new Request('https://finance-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:'/holdings/acme/finance/setup/'+suffix+'?tab=company#identity'}),{headers:{host:'finance-dev-erpx.jesadakorn.com'}});
 const response=await productWorkspaceRequest(request());expect(response.status).toBe(303);expect(response.headers.get('location')).toContain('/finance/setup/thai-tax?tab=company#identity');expect(await response.text()).not.toContain('private-current-capability-revision');expect(mocks.legal).toHaveBeenCalledWith(expect.objectContaining({holdingId:'h001',companyId:'c2',assignmentId:'assignment',assignmentVersion:1}));expect(mocks.select).not.toHaveBeenCalled();
 expect((await productWorkspaceRequest(request('chart'))).status).toBe(403);mocks.legal.mockResolvedValue(null);expect((await productWorkspaceRequest(request())).status).toBe(403);
});


it('accepts only the same-origin versioned role form for a Front backoffice selection and never redirects a native mutation across hosts',async()=>{
 const app='pos',account=access();account.assignments[0].modules=[app];mocks.access.mockResolvedValue(account);mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['assignment',[app]]]),presentationModulesByAssignment:new Map([['assignment',[app]]]),permissionsByAssignment:new Map([['assignment',builtInPermissions('holding_owner')]]),beta:false});mocks.grants.mockResolvedValue([{companyId:'c2',packageId:app,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:front-backoffice'}]);mocks.selection.mockResolvedValue({...selected(),assignment_id:'another-role'});
 const request=(origin='https://pos-dev-erpx.jesadakorn.com')=>new Request('https://pos-dev-erpx.jesadakorn.com/launch',{method:'POST',headers:{host:'pos-dev-erpx.jesadakorn.com',origin,'content-type':'application/x-www-form-urlencoded'},body:new URLSearchParams({companyId:'c2',next:'/holdings/acme/pos/settings?tab=accounting',assignmentId:'assignment',assignmentVersion:'1',revision:'3'})});
 const response=await launchPost(request());expect(response.status).toBe(303);const next=new URL(response.headers.get('location')!);expect(next.origin).toBe('https://pos-dev-erpx.jesadakorn.com');expect(next.pathname).toBe('/launch');expect(next.searchParams.get('next')).toBe('/holdings/acme/pos/settings?tab=accounting');expect(mocks.select).toHaveBeenCalledOnce();expect(mocks.front).not.toHaveBeenCalled();
 mocks.select.mockClear();const rejected=await launchPost(request('https://evil.test'));expect(rejected.status).toBe(403);expect(rejected.headers.get('location')).toBeNull();expect(mocks.select).not.toHaveBeenCalled();
 const native=await launchPost(new Request('https://pos-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:'/acme/acme-co/holding/sale'}),{method:'POST',headers:{host:'pos-dev-erpx.jesadakorn.com','content-type':'application/json'},body:'{}'}));expect(native.status).toBe(405);expect(native.headers.get('location')).toBeNull();
});


it('offers only exact notifications to an actually purchased foundation without discovering Work or changing the live role',async()=>{
 const account=access(),facts={grants:[{companyId:'c2',packageId:'pos',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:notification-foundation'}],history:[]};account.assignments[0].modules=['pos'];const authority={modulesByAssignment:new Map([['assignment',['pos']]]),presentationModulesByAssignment:new Map([['assignment',['pos']]]),permissionsByAssignment:new Map([['assignment',['pos.read']]]),beta:false};let ready=true;const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>facts,ready:async()=>ready,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('work','c2','/holdings/acme/inbox/notifications?view=all&tag=one&tag=two#update')!,page=await service.read('work',input);expect(page.target).toBe(input.target);expect(page.sourceRecord).toBe(true);expect(page.options).toMatchObject([{assignmentId:'assignment',purchasePurpose:'history',sourceRecord:true}]);expect(mocks.select).not.toHaveBeenCalled();
 for(const destination of ['home','inbox','inbox/messages','inbox/teams','work/actions','calendar/mine','inbox/notifications/extra'])await expect(service.read('work',productWorkspaceTarget('work','c2','/holdings/acme/'+destination)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 ready=false;await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});ready=true;facts.grants=[];await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});facts.grants=[{companyId:'foreign',packageId:'pos',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:foreign-notification'}];await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
});

it('offers only an exact accepted Recruitment handoff to the existing Company HR role without selecting it or granting People',async()=>{
 const account=access();account.assignments[0].modules=['people_payroll'];
 const authority={modulesByAssignment:new Map([['assignment',['people_payroll']]]),presentationModulesByAssignment:new Map([['assignment',['people_payroll']]]),permissionsByAssignment:new Map([['assignment',['people.manage']]]),beta:false};
 const source=vi.fn().mockResolvedValue({capability:'recruit.offers',purchasePurpose:'history'}),ready=vi.fn().mockResolvedValue(true);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'recruit',packageVersion:1,state:'draining',startsAt:0,endsAt:null,version:2,reason:'Fixture',evidence:'test:accepted-hire-entry'}],history:[]}),ready,talentHireSourceEntry:source,selection:async()=>({...selected(),assignment_id:'another-role'}),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('people','c2','/holdings/acme/people-organization/people/hire-handoffs/actual-accepted-handoff/overview?tag=one&tag=two#history')!,state=await service.read('people',input);
 expect(state.target).toBeNull();expect(state.options).toMatchObject([{assignmentId:'assignment',assignmentVersion:1,target:input.target,purchasePurpose:'history',sourceRecord:true}]);expect(source).toHaveBeenCalledWith(account,account.assignments[0],{id:'actual-accepted-handoff'});expect(ready).toHaveBeenCalledWith(account,'people_payroll');expect(ready).toHaveBeenCalledWith(account,'talent');
 source.mockClear();for(const path of ['people-organization/people/hire-handoffs','people-organization/people/hire-handoffs/new','people-organization/people/hire-handoffs/consume','people-organization/people/directory/current','people-organization/people/payroll/dashboard'])await expect(service.read('people',productWorkspaceTarget('people','c2',`/holdings/acme/${path}`)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 expect(source).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['people_payroll']);
});
it('rechecks the accepted hire source, Company HR authority and both owner releases before bounded People entry',async()=>{
 const account=access(),authority={modulesByAssignment:new Map([['assignment',['people_payroll']]]),presentationModulesByAssignment:new Map([['assignment',['people_payroll']]]),permissionsByAssignment:new Map([['assignment',['people.manage']]]),beta:false};
 let available=true,unreleased='';const source=vi.fn(async()=>available?{capability:'recruit.offers',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[],history:[]}),ready:async(_access,id)=>id!==unreleased,talentHireSourceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('people','c2','/holdings/acme/people-organization/people/hire-handoffs/actual-accepted-handoff')!;expect((await service.read('people',input)).sourceRecord).toBe(true);
 available=false;await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});available=true;
 for(const id of ['people_payroll','talent']){unreleased=id;await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});}unreleased='';authority.permissionsByAssignment.set('assignment',[]);await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(mocks.select).not.toHaveBeenCalled();
});

it('retains only exact candidate privacy entry for original Company Legal authority without granting the Recruitment workspace',async()=>{
 const account=access();account.assignments[0].modules=['legal'];
 const authority={modulesByAssignment:new Map([['assignment',['legal']]]),presentationModulesByAssignment:new Map([['assignment',['legal']]]),permissionsByAssignment:new Map([['assignment',['legal.hold']]]),beta:false};
 let available=true,released=true;const source=vi.fn(async()=>available?{capability:'recruit.candidates',purchasePurpose:'history' as const}:null),ready=vi.fn(async()=>released);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[],history:[]}),ready,talentCandidatePrivacySourceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('recruit','c2','/holdings/acme/people-organization/talent/candidates/actual-candidate/privacy/overview?tag=one&tag=two#history')!;
 expect((await service.read('recruit',input))).toMatchObject({target:input.target,sourceRecord:true});expect(source).toHaveBeenCalledWith(account,account.assignments[0],{id:'actual-candidate'});expect(ready).toHaveBeenCalledWith(account,'talent');
 available=false;await expect(service.read('recruit',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});available=true;released=false;await expect(service.read('recruit',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});released=true;authority.permissionsByAssignment.set('assignment',[]);await expect(service.read('recruit',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 source.mockClear();for(const path of ['people-organization/talent','people-organization/talent/candidates','people-organization/talent/candidates/new/privacy','people-organization/talent/candidates/actual-candidate/privacy/documents'])await expect(service.read('recruit',productWorkspaceTarget('recruit','c2',`/holdings/acme/${path}`)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 expect(source).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['legal']);
});

it('offers only exact owner-proven included conversations without Work discovery or switching a different live role',async()=>{
 const account=access();account.assignments[0].modules=['pms'];const source=vi.fn(async()=>({purchasePurpose:'history' as const})),ready=vi.fn(async()=>true),authority={modulesByAssignment:new Map([['assignment',['pms']]]),presentationModulesByAssignment:new Map([['assignment',['pms']]]),permissionsByAssignment:new Map([['assignment',['pms.read','pms.housekeeping']]]),beta:false};
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'pms',packageVersion:1,state:'draining',startsAt:0,endsAt:null,version:2,reason:'Fixture',evidence:'test:included-post-entry'}],history:[]}),ready,socialPostSourceEntry:source,selection:async()=>({...selected(),assignment_id:'other-live-role'}),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('work','c2','/holdings/acme/posts/actual-source-post?tag=one&tag=two#comments')!,page=await service.read('work',input);expect(page.target).toBeNull();expect(page.options).toEqual([{assignmentId:'assignment',assignmentVersion:1,role:'holding_owner',label:'holding owner',target:input.target,purchasePurpose:'history',sourceRecord:true}]);expect(source).toHaveBeenCalledWith(account,account.assignments[0],{kind:'post',id:'actual-source-post'});expect(ready).toHaveBeenCalledWith(account,'collaboration');expect(mocks.select).not.toHaveBeenCalled();
 source.mockClear();for(const path of ['home','posts','posts/new','posts/actual-source-post/comments','work/actions','calendar/mine']){expect(socialPostSourceEntry(path)).toBeNull();const target=productWorkspaceTarget('work','c2','/holdings/acme/'+path);if(target)await expect(service.read('work',target)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});else expect(target).toBeNull();}expect(source).not.toHaveBeenCalled();
});
it('rechecks exact included conversation source and release before normal role selection',async()=>{
 const account=access(),source=vi.fn<NonNullable<Parameters<typeof createProductWorkspace>[0]['socialPostSourceEntry']>>().mockResolvedValue({purchasePurpose:'history'});let released=true;
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',[]]]),beta:false}),purchaseFacts:async()=>({grants:[],history:[]}),ready:async()=>released,socialPostSourceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)}),input=productWorkspaceTarget('work','c2','/holdings/acme/posts/actual-source-post')!;
 expect(await service.read('work',input)).toMatchObject({target:input.target,sourceRecord:true});source.mockResolvedValue(null);await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});source.mockResolvedValue({purchasePurpose:'history'});released=false;await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(mocks.select).not.toHaveBeenCalled();for(const value of ['posts/..','posts/.','posts/new','posts/a?source=project','posts/a/b'])expect(socialPostSourceEntry(value)).toBeNull();
});
it('routes exact personal workforce entry through its owner even beside full Work purchase without inventing employee scope',async()=>{
 const account=access();account.assignments[0].modules=[];const source=vi.fn<NonNullable<Parameters<typeof createProductWorkspace>[0]['workforceSelfServiceEntry']>>().mockResolvedValue({capability:'people.workforce',purchasePurpose:'history'}),ready=vi.fn(async()=>true),facts={grants:[{companyId:'c2',packageId:'work',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:exact-self-owner'}],history:[]};
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',[]]]),beta:false}),purchaseFacts:async()=>facts,ready,workforceSelfServiceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)}),input=productWorkspaceTarget('work','c2','/holdings/acme/leave?tag=one&tag=two#history')!;
 expect(await service.read('work',input)).toMatchObject({target:input.target,sourceRecord:true});expect(source).toHaveBeenCalledWith(account,account.assignments[0],{kind:'leave',view:'list'});expect(ready).toHaveBeenCalledWith(account,'people_payroll');source.mockResolvedValue(null);await expect(service.read('work',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});
it('opens a normal purchased Social post for the original module-free Work role through collaboration release without requiring an included foreign source',async()=>{
 const account=access();account.assignments[0].modules=[];const source=vi.fn(),ready=vi.fn(async()=>true),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',[]]]),beta:false}),purchaseFacts:async()=>({grants:['work','work-social'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:normal-social-product'})),history:[]}),ready,socialPostSourceEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)}),input=productWorkspaceTarget('work','c2','/holdings/acme/posts/actual-ordinary-post?tag=one&tag=two#comments')!;
 expect(await service.read('work',input)).toMatchObject({target:input.target,sourceRecord:false});expect(source).not.toHaveBeenCalled();expect(ready).toHaveBeenCalledWith(account,'collaboration');expect(account.assignments[0].modules).toEqual([]);expect(mocks.select).not.toHaveBeenCalled();
});

it('opens only actual Supply posting setup for a Company Finance-only role without discovering Supply management',async()=>{
 const account=access(),authority={modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',['finance.read','finance.write']]]),beta:false};
 const owner=vi.fn(async()=>({revision:'private-owner-posting-revision',purchasePurpose:'history' as const})),ready=vi.fn(async()=>true);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>authority,purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:supply-posting-entry'}],history:[]}),ready,commercePostingSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('supply','c2','/holdings/acme/purchasing/setup/posting?tag=one&tag=two#accounts')!;
 const state=await service.read('supply',input);expect(state).toMatchObject({target:input.target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,purchasePurpose:'history',sourceRecord:true}]});expect(owner).toHaveBeenCalledWith(account,account.assignments[0]);expect(ready).toHaveBeenCalledWith(account,'finance');expect(ready).toHaveBeenCalledWith(account,'procurement');expect(account.assignments[0].modules).toEqual(['finance']);expect(mocks.select).not.toHaveBeenCalled();
 for(const destination of ['purchasing/orders','purchasing/suppliers','purchasing/setup','purchasing/setup/posting/extra','inventory-quality/stock'])await expect(service.read('supply',productWorkspaceTarget('supply','c2','/holdings/acme/'+destination)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 expect(productWorkspaceTarget('finance','c2',input.target)).toBeNull();expect(productWorkspaceTarget('sales','c2',input.target)).toBeNull();
 account.assignments[0].modules=[];authority.modulesByAssignment.set('assignment',[]);authority.presentationModulesByAssignment.set('assignment',[]);expect((await service.read('supply',input)).sourceRecord).toBe(true);expect(account.assignments[0].modules).toEqual([]);
});
it('rechecks original Finance authority, current owner eligibility and both releases on the exact Supply setup entry',async()=>{
 const account=access();let permission=['finance.read','finance.write'],available=true,released=true;const owner=vi.fn(async()=>available?{revision:'current',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',permission]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:supply-posting-authority'}],history:[]}),ready:async(_access,id)=>id!=='procurement'||released,commercePostingSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('supply','c2','/holdings/acme/purchasing/setup/posting')!;
 expect((await service.read('supply',input)).sourceRecord).toBe(true);available=false;await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});available=true;
 permission=['finance.read'];owner.mockClear();await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();permission=['finance.read','finance.write'];released=false;await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});released=true;
 account.assignments[0].version=2;expect((await service.read('supply',input)).options).toMatchObject([{assignmentVersion:2}]);expect(owner).toHaveBeenLastCalledWith(account,expect.objectContaining({version:2}));
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};owner.mockClear();await expect(service.read('supply',productWorkspaceTarget('supply','c2','/holdings/acme/branches/main/purchasing/setup/posting')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();
});

it('uses the normal Supply launch and original Account session for exact Finance setup without exposing owner revisions',async()=>{
 mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'supply',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:supply-posting-adapter'}]);mocks.posting.mockResolvedValue({revision:'private-native-posting-revision',purchasePurpose:'history'});
 const target='/holdings/acme/purchasing/setup/posting?tag=one&tag=two#accounts',request=new Request('https://supply-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:target}),{headers:{host:'supply-dev-erpx.jesadakorn.com'}});
 const response=await productWorkspaceRequest(request);expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://supply-dev-erpx.jesadakorn.com'+target);expect(response.headers.get('cache-control')).toBe('private, no-store');expect(await response.text()).not.toContain('private-native-posting-revision');expect(mocks.posting).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({companyId:'c2',userId:'user',assignmentId:'assignment',assignmentVersion:1,role:'holding_owner',sessionId:'session',branchId:null,departmentId:null}));expect(mocks.select).not.toHaveBeenCalled();
});

it('allows the original Company rate approver into exact Supply preparation while retaining source authority and route limits',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read','finance.approve'],eligible=true;
 const owner=vi.fn(async()=>eligible?{revision:'private-rate-approver-source',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'supply',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:supply-rate-approver'}],history:[]}),ready:async()=>true,commercePostingSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('supply','c2','/holdings/acme/purchasing/setup/posting?step=rate#preparation')!;
 expect(await service.read('supply',input)).toMatchObject({sourceRecord:true,target:input.target});expect(owner).toHaveBeenCalledWith(account,account.assignments[0]);
 for(const destination of ['purchasing/orders','purchasing/setup','purchasing/setup/posting/extra'])await expect(service.read('supply',productWorkspaceTarget('supply','c2','/holdings/acme/'+destination)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 for(const missing of [['finance.read'],['finance.approve'],[]]){permissions=missing;owner.mockClear();await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.approve'];eligible=false;await expect(service.read('supply',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});

it('opens exact Sales posting setup for Company Finance preparation or approval without Supply or full Finance discovery',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read','finance.write'],eligible=true,released=true;
 const source=vi.fn(async()=>eligible?{revision:'private-sales-posting-source',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_account:unknown,id:string)=>id!=='sales'||released);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'sales',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:sales-posting-only'}],history:[]}),ready,salesPostingSetupEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('sales','c2','/holdings/acme/customers-sales/sales/settings/posting?tag=one&tag=two#defaults')!;
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];expect(await service.read('sales',input)).toMatchObject({target:input.target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,sourceRecord:true}]});expect(source).toHaveBeenLastCalledWith(account,account.assignments[0]);}
 expect(ready).toHaveBeenCalledWith(account,'sales');expect(ready).toHaveBeenCalledWith(account,'finance');expect(ready).not.toHaveBeenCalledWith(account,'procurement');
 for(const target of ['customers-sales/sales/settings','customers-sales/sales/settings/posting/extra','customers-sales/sales/documents/sales'])await expect(service.read('sales',productWorkspaceTarget('sales','c2','/holdings/acme/'+target)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 for(const missing of [['finance.read'],['finance.approve'],[]]){permissions=missing;source.mockClear();await expect(service.read('sales',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(source).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.approve'];eligible=false;await expect(service.read('sales',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;released=false;await expect(service.read('sales',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});released=true;
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};source.mockClear();await expect(service.read('sales',productWorkspaceTarget('sales','c2','/holdings/acme/branches/main/customers-sales/sales/settings/posting')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(source).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});


it('uses the normal Sales launch and original Account session for exact posting setup with no owner revision disclosure',async()=>{
 mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'sales',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:sales-posting-adapter'}]);mocks.salesPosting.mockResolvedValue({revision:'private-native-sales-posting-revision',purchasePurpose:'history'});
 const target='/holdings/acme/customers-sales/sales/settings/posting?tag=one&tag=two#defaults',request=new Request('https://sales-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:target}),{headers:{host:'sales-dev-erpx.jesadakorn.com'}});
 const response=await productWorkspaceRequest(request);expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://sales-dev-erpx.jesadakorn.com'+target);expect(response.headers.get('cache-control')).toBe('private, no-store');expect(await response.text()).not.toContain('private-native-sales-posting-revision');expect(mocks.salesPosting).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({companyId:'c2',userId:'user',assignmentId:'assignment',assignmentVersion:1,role:'holding_owner',sessionId:'session',branchId:null,departmentId:null}));expect(mocks.select).not.toHaveBeenCalled();
});


it('parses only the exact customer receipt owner destination independently of payment kinds',()=>{
 expect(salesCustomerReceiptEntry('finance/documents/sales/receipts/real-receipt')).toEqual({id:'real-receipt'});
 expect(financeDocumentEntry('finance/documents/sales/receipts/real-receipt')).toBeNull();
 for(const suffix of ['', '/', '/..', '/.', '/receipt/overview', '/receipt/export', '/receipt/extra', '/%2fprivate', '/receipt?source=other', '/receipt#private', '/receipt\\private'])expect(salesCustomerReceiptEntry('finance/documents/sales/receipts'+suffix)).toBeNull();
});
it('retains only owner-proven Sales receipt entry with current Finance permission and release',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read'],eligible=true,released=true;
 const source=vi.fn(async()=>eligible?{capability:'finance.sales-posting',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[],history:[]}),ready:async()=>released,salesCustomerReceiptEntry:source,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const target='/holdings/acme/finance/documents/sales/receipts/original?tag=one&tag=two#history',input=productWorkspaceTarget('finance','c2',target)!;
 expect(await service.read('finance',input)).toMatchObject({target,sourceRecord:true});expect(source).toHaveBeenCalledWith(account,account.assignments[0],{id:'original'});
 eligible=false;await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 permissions=[];source.mockClear();await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(source).not.toHaveBeenCalled();permissions=['finance.read'];released=false;await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});released=true;
 for(const path of ['finance/documents/sales/receipts','finance/documents/sales/receipts/original/export','finance/documents/sales/receipts/other/extra'])await expect(service.read('finance',productWorkspaceTarget('finance','c2','/holdings/acme/'+path)!)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});
it('uses normal Finance launch for a Sales receipt with the actual Account session and no private source disclosure',async()=>{
 mocks.grants.mockResolvedValue([]);mocks.salesReceipt.mockResolvedValue({capability:'finance.sales-posting',purchasePurpose:'history',privateReceipt:'private-owner-snapshot'});
 const target='/holdings/acme/finance/documents/sales/receipts/original?tag=one&tag=two#history';
 const response=await productWorkspaceRequest(new Request('https://finance-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next:target}),{headers:{host:'finance-dev-erpx.jesadakorn.com'}}));
 expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://finance-dev-erpx.jesadakorn.com'+target);expect(response.headers.get('cache-control')).toBe('private, no-store');expect(await response.text()).not.toContain('private-owner-snapshot');
 expect(mocks.salesReceipt).toHaveBeenCalledWith(expect.anything(),expect.objectContaining({companyId:'c2',userId:'user',assignmentId:'assignment',assignmentVersion:1,sessionId:'session',branchId:null,departmentId:null}),'original');expect(mocks.select).not.toHaveBeenCalled();
});

const initialStockEntryMock=vi.hoisted(()=>vi.fn());
vi.mock("@erpx/server-inventory/public/initial-physical-stock-count",()=>({initialPhysicalStockCountProductEntry:initialStockEntryMock}));
it('keeps the actual Stock role on the exact included Sales entry without granting Sales discovery or changing the selected role',async()=>{
 initialStockEntryMock.mockReset().mockResolvedValue({capability:'inventory.stock-core',purchasePurpose:'history'});
 const value=access();value.assignments=[{...value.assignments[0],role:'warehouse_manager',modules:['inventory']}];mocks.access.mockResolvedValue(value);
 mocks.modules.mockResolvedValue({modulesByAssignment:new Map([['assignment',['inventory']]]),presentationModulesByAssignment:new Map([['assignment',['inventory']]]),permissionsByAssignment:new Map([['assignment',['inventory.read','inventory.prepare']]]),beta:false});
 mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'sales',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual purchased Sales fixture',evidence:'test:fixture'}]);
 const next=`/holdings/acme/${(await import('@erpx/server-commercial/public/rules/product-registry')).product('sales')!.landing}?tag=one&tag=two#original`,request=new Request('https://sales-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next}),{headers:{host:'sales-dev-erpx.jesadakorn.com'}}),response=await productWorkspaceRequest(request);
 expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://sales-dev-erpx.jesadakorn.com/holdings/acme/customers-sales/sales/settings/initial-stock?tag=one&tag=two#original');
 expect(initialStockEntryMock).toHaveBeenCalledWith({},expect.objectContaining({companyId:'c2',assignmentId:'assignment',assignmentVersion:1,role:'warehouse_manager',sessionId:'session'}),{});expect(mocks.select).not.toHaveBeenCalled();
 initialStockEntryMock.mockResolvedValue(null);const denied=await productWorkspaceRequest(request);expect(denied.status).toBe(403);expect(denied.headers.get('set-cookie')).toBeNull();expect(mocks.select).not.toHaveBeenCalled();
});
it('requires exact native source entry even for a Sales-visible Company and preserves source role selection',async()=>{
 initialStockEntryMock.mockReset().mockResolvedValue(null);mocks.grants.mockResolvedValue([{companyId:'c2',packageId:'sales',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:fixture'}]);
 const next='/holdings/acme/customers-sales/sales/settings/initial-stock/count-original',request=new Request('https://sales-dev-erpx.jesadakorn.com/launch?'+new URLSearchParams({companyId:'c2',next}),{headers:{host:'sales-dev-erpx.jesadakorn.com'}});
 expect((await productWorkspaceRequest(request)).status).toBe(403);expect(mocks.select).not.toHaveBeenCalled();
 initialStockEntryMock.mockResolvedValue({capability:'inventory.stock-core',purchasePurpose:'history'});const response=await productWorkspaceRequest(request);expect(response.status).toBe(303);expect(response.headers.get('location')).toBe('https://sales-dev-erpx.jesadakorn.com'+next);expect(initialStockEntryMock).toHaveBeenCalledWith({},expect.objectContaining({companyId:'c2',assignmentId:'assignment'}),{countId:'count-original'});
});


vi.mock("@erpx/server-manufacturing/public/posting-setup",()=>({manufacturingPostingSetupProductEntry:vi.fn()}));
it('keeps Manufacturing-only Finance setup entry exact and owner-proven with the original role, query and fragment',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read','finance.write'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{revision:'private-mfg-setup-revision',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_access:unknown,id:string)=>id!==unreleased);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'manufacturing',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:mfg-finance-entry'}],history:[]}),ready,manufacturingPostingSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const target='/holdings/acme/production/manufacturing/settings/posting?tag=one&tag=two#profile',input=productWorkspaceTarget('manufacturing','c2',target)!;
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];expect(await service.read('manufacturing',input)).toMatchObject({target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,sourceRecord:true,purchasePurpose:'history'}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0]);}
 expect(ready).toHaveBeenCalledWith(account,'manufacturing');expect(ready).toHaveBeenCalledWith(account,'inventory');expect(ready).toHaveBeenCalledWith(account,'finance');
 for(const permission of [[],['finance.read'],['finance.approve'],['records.read']]){permissions=permission;owner.mockClear();await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.approve'];beta=true;owner.mockClear();await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 for(const release of ['manufacturing','inventory','finance']){unreleased=release;owner.mockClear();await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 eligible=false;await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const path of ['production/manufacturing/settings','production/manufacturing/settings/posting/extra','production/manufacturing/orders','production/planning'])await expect(service.read('manufacturing',productWorkspaceTarget('manufacturing','c2','/holdings/acme/'+path)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 const {product}=await import('@erpx/server-commercial/public/rules/product-registry');const normal=productWorkspaceTarget('manufacturing','c2','/holdings/acme/'+product('manufacturing')!.landing+'?tag=one&tag=two#profile')!;expect(await service.read('manufacturing',normal)).toMatchObject({target,sourceRecord:true});
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};owner.mockClear();await expect(service.read('manufacturing',productWorkspaceTarget('manufacturing','c2','/holdings/acme/branches/main/production/manufacturing/settings/posting')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});

it('requires named Manufacturing setup source and all releases when the ordinary module landing resolves to Finance setup',async()=>{
 const account=access();account.assignments[0].modules=['manufacturing'];let released='',eligible=true;
 const owner=vi.fn(async()=>eligible?{revision:'private-owner-image',purchasePurpose:'history' as const}:null),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['manufacturing']]]),presentationModulesByAssignment:new Map([['assignment',['manufacturing']]]),permissionsByAssignment:new Map([['assignment',['finance.read','finance.write']]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'manufacturing',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:mfg-ordinary-finance-landing'}],history:[]}),ready:async(_access,id)=>id!==released,manufacturingPostingSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('manufacturing','c2','/holdings/acme/production/manufacturing?tag=one&tag=two#profile')!;
 expect(await service.read('manufacturing',input)).toMatchObject({target:'/holdings/acme/production/manufacturing/settings/posting?tag=one&tag=two#profile',sourceRecord:true});
 eligible=false;await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const moduleId of ['manufacturing','inventory','finance']){released=moduleId;owner.mockClear();await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['manufacturing']);
});


vi.mock("@erpx/server-inventory/public/manufacturing-initial-physical-stock-count",()=>({manufacturingInitialPhysicalStockCountProductEntry:vi.fn()}));
it('keeps Manufacturing physical Counts on the actual Stock role with exact native source entry and retained id authority',async()=>{
 const account=access();account.assignments=[{...account.assignments[0],role:'warehouse_manager',modules:['inventory']}];let permissions=['inventory.read','inventory.prepare'],eligible=true,deniedRelease='';
 const owner=vi.fn(async(_account:unknown,_assignment:unknown,target:{countId?:string})=>eligible&&(target.countId||permissions.includes('inventory.prepare'))?{capability:'inventory.stock-core',purchasePurpose:'history' as const}:null),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['inventory']]]),presentationModulesByAssignment:new Map([['assignment',['inventory']]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'manufacturing',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:mfg-stock-role'}],history:[]}),ready:async(_access,id)=>id!==deniedRelease,manufacturingInitialPhysicalStockCountEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const target='/holdings/acme/production/manufacturing/settings/initial-stock?tag=one&tag=two#draft',input=productWorkspaceTarget('manufacturing','c2',target)!;expect(await service.read('manufacturing',input)).toMatchObject({target,sourceRecord:true});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],{});
 const normal=productWorkspaceTarget('manufacturing','c2','/holdings/acme/production/manufacturing?tag=one&tag=two#draft')!;expect(await service.read('manufacturing',normal)).toMatchObject({target,sourceRecord:true});
 eligible=false;await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});await expect(service.read('manufacturing',normal)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const moduleId of ['manufacturing','inventory','finance']){deniedRelease=moduleId;owner.mockClear();await expect(service.read('manufacturing',normal)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}deniedRelease='';
 permissions=['inventory.read'];const retained=productWorkspaceTarget('manufacturing','c2','/holdings/acme/production/manufacturing/settings/initial-stock/count-original?tag=one&tag=two#history')!;expect(await service.read('manufacturing',retained)).toMatchObject({target:retained.target,sourceRecord:true});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],{countId:'count-original'});await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 permissions=['inventory.read','inventory.prepare'];account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};const branch=productWorkspaceTarget('manufacturing','c2','/holdings/acme/branches/main/production/manufacturing/settings/initial-stock')!;expect(await service.read('manufacturing',branch)).toMatchObject({target:branch.target,sourceRecord:true});
 account.scope={...account.scope,kind:'department',departmentId:'ops',departmentSlug:'ops',departmentName:'Ops'};owner.mockClear();await expect(service.read('manufacturing',productWorkspaceTarget('manufacturing','c2','/holdings/acme/branches/main/departments/ops/production/manufacturing/settings/initial-stock')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['inventory']);
});
it('requires the Count owner proof when the ordinary Manufacturing-module registry chooses the Stock setup landing',async()=>{
 const account=access();account.assignments[0].modules=['manufacturing'];let eligible=true,released='';const owner=vi.fn(async()=>eligible?{capability:'inventory.stock-core',purchasePurpose:'history' as const}:null);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['manufacturing']]]),presentationModulesByAssignment:new Map([['assignment',['manufacturing']]]),permissionsByAssignment:new Map([['assignment',['inventory.read','inventory.prepare']]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'manufacturing',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:mfg-stock-discovery'}],history:[]}),ready:async(_access,id)=>id!==released,manufacturingInitialPhysicalStockCountEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)}),input=productWorkspaceTarget('manufacturing','c2','/holdings/acme/production/manufacturing')!;
 expect(await service.read('manufacturing',input)).toMatchObject({target:'/holdings/acme/production/manufacturing/settings/initial-stock',sourceRecord:true});eligible=false;await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;for(const id of ['inventory','finance']){released=id;owner.mockClear();await expect(service.read('manufacturing',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}expect(mocks.select).not.toHaveBeenCalled();
});

it('keeps POS-only Finance setup entry exact and owner-proven with the original role, query and fragment',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read','finance.write'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{revision:'private-pos-setup-revision',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_access:unknown,id:string)=>id!==unreleased);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'pos',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:pos-finance-entry'}],history:[]}),ready,posStartupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const target='/holdings/acme/pos/settings/startup?tag=one&tag=two#profile',input=productWorkspaceTarget('pos','c2',target)!;
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];expect(await service.read('pos',input)).toMatchObject({target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,sourceRecord:true,purchasePurpose:'history'}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0]);}
 expect(ready).toHaveBeenCalledWith(account,'pos');expect(ready).toHaveBeenCalledWith(account,'inventory');expect(ready).toHaveBeenCalledWith(account,'finance');
 for(const permission of [[],['finance.read'],['finance.approve'],['records.read']]){permissions=permission;owner.mockClear();await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.approve'];beta=true;owner.mockClear();await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 for(const release of ['pos','inventory','finance']){unreleased=release;owner.mockClear();await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 eligible=false;await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const path of ['pos/settings/startup/extra','pos/catalog','pos/registers'])await expect(service.read('pos',productWorkspaceTarget('pos','c2','/holdings/acme/'+path)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 const settings=productWorkspaceTarget('pos','c2','/holdings/acme/pos/settings?tag=one&tag=two#profile')!;expect(await service.read('pos',settings)).toMatchObject({target:settings.target,sourceRecord:true});
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};owner.mockClear();await expect(service.read('pos',productWorkspaceTarget('pos','c2','/holdings/acme/branches/main/pos/settings/startup')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});


it('keeps the POS Stock startup entry scoped and source proven without opening ordinary Finance settings',async()=>{
 const account=access();account.assignments[0]={...account.assignments[0],role:'warehouse_manager',modules:[]};let permissions=['inventory.read','inventory.prepare'];
 const owner=vi.fn(async()=>({revision:'private-stock-proof',purchasePurpose:'history' as const})),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'pos',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:pos-stock-entry'}],history:[]}),ready:async()=>true,posStartupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('pos','c2','/holdings/acme/pos/settings/startup?tag=one&tag=two#stock')!;expect(await service.read('pos',input)).toMatchObject({target:input.target,sourceRecord:true});
 await expect(service.read('pos',productWorkspaceTarget('pos','c2','/holdings/acme/pos/settings')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};const branch=productWorkspaceTarget('pos','c2','/holdings/acme/branches/main/pos/settings/startup')!;expect(await service.read('pos',branch)).toMatchObject({target:branch.target,sourceRecord:true});
 for(const values of [[],['inventory.read'],['inventory.prepare']]){permissions=values;owner.mockClear();await expect(service.read('pos',branch)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['inventory.read','inventory.prepare'];account.scope={...account.scope,kind:'department',departmentId:'ops',departmentSlug:'ops',departmentName:'Ops'};owner.mockClear();await expect(service.read('pos',productWorkspaceTarget('pos','c2','/holdings/acme/branches/main/departments/ops/pos/settings/startup')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});


it('opens only source-proven Assets accounting for the original Company Finance role and preserves the requested record',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read','finance.write'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{revision:'private-assets-source',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_account:unknown,id:string)=>id!==unreleased);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'assets',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:assets-financial-entry'}],history:[]}),ready,assetsFinancialEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const permission of ['finance.write','finance.approve'])for(const suffix of ['','/asset-real']){
  permissions=['finance.read',permission];const target='/holdings/acme/services/assets/accounting'+suffix+'?tag=one&tag=two#editor',input=productWorkspaceTarget('assets','c2',target)!;
  expect(await service.read('assets',input)).toMatchObject({target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,purchasePurpose:'history',sourceRecord:true}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],suffix?{assetId:'asset-real'}:{});
 }
 const input=productWorkspaceTarget('assets','c2','/holdings/acme/services/assets/accounting')!;
 permissions=['finance.read'];expect(await service.read('assets',input)).toMatchObject({target:input.target,sourceRecord:true});
 for(const denied of [[],['finance.approve'],['records.read']]){permissions=denied;owner.mockClear();await expect(service.read('assets',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.write'];beta=true;owner.mockClear();await expect(service.read('assets',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 for(const release of ['assets','finance']){unreleased=release;owner.mockClear();await expect(service.read('assets',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 eligible=false;await expect(service.read('assets',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 const root=productWorkspaceTarget('assets','c2','/holdings/acme/services/assets/register?tag=one&tag=two#editor')!;expect(await service.read('assets',root)).toMatchObject({target:'/holdings/acme/services/assets/accounting?tag=one&tag=two#editor',sourceRecord:true});
 for(const path of ['services/assets/accounting/asset-real/extra','services/assets/physical-real']){owner.mockClear();await expect(service.read('assets',productWorkspaceTarget('assets','c2','/holdings/acme/'+path)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};owner.mockClear();await expect(service.read('assets',productWorkspaceTarget('assets','c2','/holdings/acme/branches/main/services/assets/accounting')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});

it('keeps POS initial Stock entry on the original Stock role and exact Count without POS module synthesis',async()=>{
 const account=access();account.assignments[0]={...account.assignments[0],role:'warehouse_manager',modules:['inventory']};let permissions=['inventory.read','inventory.prepare'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{capability:'inventory.stock-core',purchasePurpose:'history' as const}:null),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['inventory']]]),presentationModulesByAssignment:new Map([['assignment',['inventory']]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'pos',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:pos-stock-entry'}],history:[]}),ready:async(_account,id)=>id!==unreleased,posInitialPhysicalStockCountEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const suffix of ['','/count-real%3Aoriginal']){
  const target='/holdings/acme/pos/settings/initial-stock'+suffix+'?tag=one&tag=two#evidence',input=productWorkspaceTarget('pos','c2',target)!;
  expect(await service.read('pos',input)).toMatchObject({target,sourceRecord:true});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],suffix?{countId:'count-real:original'}:{});
 }
 const input=productWorkspaceTarget('pos','c2','/holdings/acme/pos/settings/initial-stock/count-real')!;
 permissions=[];owner.mockClear();await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();permissions=['inventory.read'];
 expect(await service.read('pos',input)).toMatchObject({target:input.target,sourceRecord:true});eligible=false;await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const release of ['pos','inventory','finance']){unreleased=release;owner.mockClear();await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 beta=true;owner.mockClear();await expect(service.read('pos',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};const branch=productWorkspaceTarget('pos','c2','/holdings/acme/branches/main/pos/settings/initial-stock/count-real')!;expect(await service.read('pos',branch)).toMatchObject({target:branch.target,sourceRecord:true});
 account.scope={...account.scope,kind:'department',departmentId:'ops',departmentSlug:'ops',departmentName:'Ops'};owner.mockClear();await expect(service.read('pos',productWorkspaceTarget('pos','c2','/holdings/acme/branches/main/departments/ops/pos/settings/initial-stock/count-real')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['inventory']);
});


it('keeps Service parts Finance setup entry exact and owner-proven with the original role, query and fragment',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['finance.read','finance.write'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{revision:'private-service-setup-revision',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_access:unknown,id:string)=>id!==unreleased);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'service',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:service-finance-entry'},{companyId:'c2',packageId:'service-parts-assets',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:parts'}],history:[]}),ready,servicePartsSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const target='/holdings/acme/services/field-service/settings/parts?tag=one&tag=two#profile',input=productWorkspaceTarget('service','c2',target)!;
 for(const permission of ['finance.write','finance.approve']){permissions=['finance.read',permission];expect(await service.read('service',input)).toMatchObject({target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,sourceRecord:true,purchasePurpose:'history'}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0]);}
 expect(ready).toHaveBeenCalledWith(account,'field_service');expect(ready).toHaveBeenCalledWith(account,'inventory');expect(ready).toHaveBeenCalledWith(account,'finance');
 for(const permission of [[],['finance.read'],['finance.approve'],['records.read']]){permissions=permission;owner.mockClear();await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['finance.read','finance.approve'];beta=true;owner.mockClear();await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 for(const release of ['field_service','inventory','finance']){unreleased=release;owner.mockClear();await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 eligible=false;await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const path of ['services/field-service/settings/parts/extra','services/field-service/catalog','services/field-service/registers'])await expect(service.read('service',productWorkspaceTarget('service','c2','/holdings/acme/'+path)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 const settings=productWorkspaceTarget('service','c2','/holdings/acme/services/field-service?tag=one&tag=two#profile')!;expect(await service.read('service',settings)).toMatchObject({target:'/holdings/acme/services/field-service/settings/parts?tag=one&tag=two#profile',sourceRecord:true});
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};owner.mockClear();await expect(service.read('service',productWorkspaceTarget('service','c2','/holdings/acme/branches/main/services/field-service/settings/parts')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});


it('keeps Service initial Stock entry on the original Stock role and exact Count without buying Stock Core or synthesizing a Service role',async()=>{
 const account=access();account.assignments[0]={...account.assignments[0],role:'warehouse_manager',modules:['inventory']};let permissions=['inventory.read','inventory.prepare'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{capability:'service.parts-assets',purchasePurpose:'history' as const}:null),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['inventory']]]),presentationModulesByAssignment:new Map([['assignment',['inventory']]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'service',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:service-stock-entry'},{companyId:'c2',packageId:'service-parts-assets',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:parts'}],history:[]}),ready:async(_account,id)=>id!==unreleased,serviceInitialPhysicalStockCountEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const suffix of ['','/count-real%3Aoriginal']){
  const target='/holdings/acme/services/field-service/settings/initial-stock'+suffix+'?tag=one&tag=two#evidence',input=productWorkspaceTarget('service','c2',target)!;
  expect(await service.read('service',input)).toMatchObject({target,sourceRecord:true});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],suffix?{countId:'count-real:original'}:{});
 }
 const input=productWorkspaceTarget('service','c2','/holdings/acme/services/field-service/settings/initial-stock/count-real')!;
 permissions=[];owner.mockClear();await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();permissions=['inventory.read'];
 expect(await service.read('service',input)).toMatchObject({target:input.target,sourceRecord:true});eligible=false;await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const release of ['field_service','inventory','finance']){unreleased=release;owner.mockClear();await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 beta=true;owner.mockClear();await expect(service.read('service',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};const branch=productWorkspaceTarget('service','c2','/holdings/acme/branches/main/services/field-service/settings/initial-stock/count-real')!;expect(await service.read('service',branch)).toMatchObject({target:branch.target,sourceRecord:true});
 account.scope={...account.scope,kind:'department',departmentId:'ops',departmentSlug:'ops',departmentName:'Ops'};owner.mockClear();await expect(service.read('service',productWorkspaceTarget('service','c2','/holdings/acme/branches/main/departments/ops/services/field-service/settings/initial-stock/count-real')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual(['inventory']);
});


it('keeps the Service parts Stock preparation entry scoped and source proven without opening ordinary Finance settings',async()=>{
 const account=access();account.assignments[0]={...account.assignments[0],role:'warehouse_manager',modules:[]};let permissions=['inventory.read','inventory.prepare'];
 const owner=vi.fn(async()=>({revision:'private-stock-proof',purchasePurpose:'history' as const})),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'service',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:service-stock-entry'},{companyId:'c2',packageId:'service-parts-assets',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:parts'}],history:[]}),ready:async()=>true,servicePartsSetupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('service','c2','/holdings/acme/services/field-service/settings/parts?tag=one&tag=two#stock')!;expect(await service.read('service',input)).toMatchObject({target:input.target,sourceRecord:true});
 await expect(service.read('service',productWorkspaceTarget('service','c2','/holdings/acme/services/field-service/settings')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 account.scope={...account.scope,kind:'branch',branchId:'main',branchSlug:'main',branchName:'Main'};const branch=productWorkspaceTarget('service','c2','/holdings/acme/branches/main/services/field-service/settings/parts')!;expect(await service.read('service',branch)).toMatchObject({target:branch.target,sourceRecord:true});
 for(const values of [[],['inventory.read'],['inventory.prepare']]){permissions=values;owner.mockClear();await expect(service.read('service',branch)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['inventory.read','inventory.prepare'];account.scope={...account.scope,kind:'department',departmentId:'ops',departmentSlug:'ops',departmentName:'Ops'};owner.mockClear();await expect(service.read('service',productWorkspaceTarget('service','c2','/holdings/acme/branches/main/departments/ops/services/field-service/settings/parts')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});

vi.mock("@erpx/server-people_payroll/public/project-worker-startup",()=>({projectWorkerStartupProductEntry:vi.fn()}));
it('keeps Projects worker preparation on the exact current Company HR role and owner-proven Person without a People module',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['people.manage','organization.manage'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{revision:'private-project-worker-authority',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_access:unknown,id:string)=>id!==unreleased);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'projects',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:projects-worker-entry'}],history:[]}),ready,projectWorkerStartupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const suffix of ['','/person-real%3Aoriginal']){
  const target='/holdings/acme/projects-routines/projects/settings/workers'+suffix+'?tag=one&tag=two#placement',input=productWorkspaceTarget('projects','c2',target)!;
  const result=await service.read('projects',input);expect(result).toMatchObject({target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,role:'holding_owner',target,sourceRecord:true,purchasePurpose:'history'}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],suffix?{personId:'person-real:original'}:{});expect(JSON.stringify(result.options)).not.toContain('private-project-worker-authority');
 }
 for(const denied of [[],['people.manage'],['organization.manage'],['records.read']]){permissions=denied;owner.mockClear();await expect(service.read('projects',productWorkspaceTarget('projects','c2','/holdings/acme/projects-routines/projects/settings/workers')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['people.manage','organization.manage'];const input=productWorkspaceTarget('projects','c2','/holdings/acme/projects-routines/projects/settings/workers/person-real')!;
 for(const release of ['projects','people_payroll']){unreleased=release;owner.mockClear();await expect(service.read('projects',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 beta=true;owner.mockClear();await expect(service.read('projects',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 eligible=false;await expect(service.read('projects',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 account.assignments[0].version=0;owner.mockClear();await expect(service.read('projects',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();account.assignments[0].version=1;
 for(const target of ['/holdings/acme/branches/main/projects-routines/projects/settings/workers','/holdings/acme/departments/team/projects-routines/projects/settings/workers']){const scoped=productWorkspaceTarget('projects','c2',target)!;account.scope=target.includes('/branches/')?{kind:'branch',holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',companyId:'c2',branchId:'main',branchSlug:'main',branchName:'Main'}:{kind:'department',holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',companyId:'c2',branchId:null,departmentId:'team',departmentSlug:'team',departmentName:'Team'};owner.mockClear();await expect(service.read('projects',scoped)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});

it('uses Projects worker preparation only as the missing ordinary landing fallback and keeps the selected scope query and fragment',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['people.manage','organization.manage'];
 const owner=vi.fn(async()=>({revision:'private-worker-source',purchasePurpose:'history' as const})),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',account.assignments[0].modules]]),presentationModulesByAssignment:new Map([['assignment',account.assignments[0].modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'projects',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:projects-worker-fallback'}],history:[]}),ready:async()=>true,projectWorkerStartupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('projects','c2','/holdings/acme/projects-routines/projects?tag=one&tag=two#worker')!;
 expect(await service.read('projects',input)).toMatchObject({target:'/holdings/acme/projects-routines/projects/settings/workers?tag=one&tag=two#worker',sourceRecord:true});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],{});
 account.assignments[0].modules=['projects'];permissions=['records.read','people.manage','organization.manage'];owner.mockClear();expect(await service.read('projects',input)).toMatchObject({target:input.target,sourceRecord:false});expect(owner).not.toHaveBeenCalled();
 permissions=['people.manage','organization.manage'];account.assignments[0].modules=[];for(const path of ['people-organization/people/directory/current','projects-routines/projects/settings/other','projects-routines/projects/settings/workers/person-real/extra']){const target=productWorkspaceTarget('projects','c2','/holdings/acme/'+path);if(target)await expect(service.read('projects',target)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});else expect(target).toBeNull();}expect(mocks.select).not.toHaveBeenCalled();
});

vi.mock("@erpx/server-people_payroll/public/learning-worker-startup",()=>({learningWorkerStartupProductEntry:vi.fn()}));
it('keeps Learning worker preparation on the exact current Company HR role and owner-proven Person without a People module',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['people.manage','organization.manage'],eligible=true,beta=false,unreleased='';
 const owner=vi.fn(async()=>eligible?{revision:'private-learning-worker-authority',purchasePurpose:'history' as const}:null),ready=vi.fn(async(_access:unknown,id:string)=>id!==unreleased);
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',[]]]),presentationModulesByAssignment:new Map([['assignment',[]]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'learn',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:learning-worker-entry'}],history:[]}),ready,learningWorkerStartupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 for(const suffix of ['','/person-real%3Aoriginal']){
  const target='/holdings/acme/knowledge/learning/settings/learners'+suffix+'?tag=one&tag=two#placement',input=productWorkspaceTarget('learn','c2',target)!;
  const result=await service.read('learn',input);expect(result).toMatchObject({target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,role:'holding_owner',target,sourceRecord:true,purchasePurpose:'history'}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],suffix?{personId:'person-real:original'}:{});expect(JSON.stringify(result.options)).not.toContain('private-learning-worker-authority');
 }
 for(const denied of [[],['people.manage'],['organization.manage'],['records.read']]){permissions=denied;owner.mockClear();await expect(service.read('learn',productWorkspaceTarget('learn','c2','/holdings/acme/knowledge/learning/settings/learners')!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 permissions=['people.manage','organization.manage'];const input=productWorkspaceTarget('learn','c2','/holdings/acme/knowledge/learning/settings/learners/person-real')!;
 for(const release of ['learning','people_payroll']){unreleased=release;owner.mockClear();await expect(service.read('learn',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}unreleased='';
 beta=true;owner.mockClear();await expect(service.read('learn',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();beta=false;
 eligible=false;await expect(service.read('learn',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 account.assignments[0].version=0;owner.mockClear();await expect(service.read('learn',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();account.assignments[0].version=1;
 for(const target of ['/holdings/acme/branches/main/knowledge/learning/settings/learners','/holdings/acme/departments/team/knowledge/learning/settings/learners']){const scoped=productWorkspaceTarget('learn','c2',target)!;account.scope=target.includes('/branches/')?{kind:'branch',holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',companyId:'c2',branchId:'main',branchSlug:'main',branchName:'Main'}:{kind:'department',holdingId:'h001',holdingSlug:'acme',holdingName:'Acme',companyId:'c2',branchId:null,departmentId:'team',departmentSlug:'team',departmentName:'Team'};owner.mockClear();await expect(service.read('learn',scoped)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 expect(mocks.select).not.toHaveBeenCalled();expect(account.assignments[0].modules).toEqual([]);
});

it('uses Learning worker preparation only as the missing ordinary landing fallback and keeps the selected scope query and fragment',async()=>{
 const account=access();account.assignments[0].modules=[];let permissions=['people.manage','organization.manage'];
 const owner=vi.fn(async()=>({revision:'private-worker-source',purchasePurpose:'history' as const})),service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',account.assignments[0].modules]]),presentationModulesByAssignment:new Map([['assignment',account.assignments[0].modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:[{companyId:'c2',packageId:'learn',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:learning-worker-fallback'}],history:[]}),ready:async()=>true,learningWorkerStartupEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 const input=productWorkspaceTarget('learn','c2','/holdings/acme/knowledge/learning?tag=one&tag=two#worker')!;
 expect(await service.read('learn',input)).toMatchObject({target:'/holdings/acme/knowledge/learning/settings/learners?tag=one&tag=two#worker',sourceRecord:true});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],{});
 account.assignments[0].modules=['learning'];permissions=['records.read','people.manage','organization.manage'];owner.mockClear();expect(await service.read('learn',input)).toMatchObject({target:input.target,sourceRecord:false});expect(owner).not.toHaveBeenCalled();
 permissions=['people.manage','organization.manage'];account.assignments[0].modules=[];for(const path of ['people-organization/people/directory/current','knowledge/learning/settings/other','knowledge/learning/settings/learners/person-real/extra']){const target=productWorkspaceTarget('learn','c2','/holdings/acme/'+path);if(target)await expect(service.read('learn',target)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});else expect(target).toBeNull();}expect(mocks.select).not.toHaveBeenCalled();
});


// Permission-based included financial entry; no People module or base Finance purchase.
it('requires the named Employee Expenses purchase and the current Finance owner proof',async()=>{
 const {productEntryCapability}=await import('@erpx/server-commercial/public/rules/product-entry-capability');
 const account=access();account.assignments[0]={...account.assignments[0],role:'custom_expense_accounting',modules:['finance']};
 let packages=['finance'],modules=['finance'],permissions=['module.finance.read','finance.read','expense.read','expense.prepare'],ownerAllowed=true;
 const owner=vi.fn(async()=>ownerAllowed?{revision:'private-employee-expense-source',purchasePurpose:'history' as const}:null),input=productWorkspaceTarget('finance','c2','/holdings/acme/finance/documents/expenses/claim-original/overview')!;
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',modules]]),presentationModulesByAssignment:new Map([['assignment',modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:packages.map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:employee-expenses-entry'})),history:[]}),ready:async()=>true,peopleFinancialEntry:owner,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)});
 expect(productEntryCapability('finance','finance/documents/expenses')).toBe('people.expenses');expect(productEntryCapability('finance','finance/documents/expenses/claim-original/overview')).toBe('people.expenses');
 await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();
 packages=['people','people-expenses'];
 expect(await service.read('finance',input)).toMatchObject({target:input.target,sourceRecord:true,options:[{purchasePurpose:'history',sourceRecord:true}]});expect(owner).toHaveBeenCalledWith(account,account.assignments[0],{kind:'expense',id:'claim-original'});
 owner.mockClear();ownerAllowed=false;await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).toHaveBeenCalledOnce();
 ownerAllowed=true;modules=[];owner.mockClear();await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();
 modules=['finance'];permissions=['module.finance.read','finance.read','expense.read'];owner.mockClear();await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();
});

it('requires the Helpdesk Portal add-on, its Support base dependency, and the existing Support owner permission',async()=>{
 const {supportTicketPortalLanding}=await import('@erpx/server-support/public/rules/ticket-portal-entry');
 const account=access();account.assignments[0]={...account.assignments[0],role:'custom_support_agent',modules:['support']};
 let packages=['support'],modules=['support'],permissions=['support.read'];
 const service=createProductWorkspace({access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',modules]]),presentationModulesByAssignment:new Map([['assignment',modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:packages.map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:helpdesk-portal-entry'})),history:[]}),ready:async()=>true,selection:async()=>selected(),select:mocks.select,now:Date.now,error:(status,code,message)=>new ApiError(status,code,message)}),input=productWorkspaceTarget('support','c2',`/holdings/acme/${supportTicketPortalLanding}/binding-original`)!;
 await expect(service.read('support',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 packages=['support-portal'];await expect(service.read('support',input)).rejects.toMatchObject({code:'PRODUCT_PACKAGE_REQUIRED'});
 packages=['support','support-portal'];expect(await service.read('support',input)).toMatchObject({target:input.target,options:[{purchasePurpose:'new-work'}]});
 permissions=[];await expect(service.read('support',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
 permissions=['support.read'];modules=[];await expect(service.read('support',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});
});

it('opens exact Expense Finance and Payroll People owner pages for the current financial assignment without expanding its modules',async()=>{
 const {peopleFinancialEntry}=await import('@erpx/server-people_payroll/public/rules/financial-product-entry');
 const account=access();account.assignments[0]={...account.assignments[0],role:'custom_financial_settlement',modules:['finance']};
 const permissions=['module.finance.read','expense.pay','payroll.payment.read','finance.pay'];
 const source=vi.fn(async()=>({revision:'private-owner-image',purchasePurpose:'history' as const}));
 const roles={modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false};
 const grants=['people','people-expenses','people-payroll'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:financial-source'}));
 const ports={access:async()=>account,modules:async()=>roles,purchaseFacts:async()=>({grants,history:[]}),ready:async()=>true,peopleFinancialEntry:source,selection:async()=>selected(),select:vi.fn(),now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)};
 const service=createProductWorkspace(ports);
 for(const [audience,path,kind,id]of [['finance','finance/documents/expenses','expense',undefined],['finance','finance/documents/expenses/e-1/overview','expense','e-1'],['people','people-organization/people/payroll/dashboard','payroll',undefined],['people','people-organization/people/payroll/runs/r-1','payroll','r-1']]as const){
  const input=productWorkspaceTarget(audience,'c2',`/holdings/acme/${path}?q=retained#work`)!;
  expect(await service.read(audience,input)).toMatchObject({target:input.target,sourceRecord:true,options:[{assignmentId:'assignment',assignmentVersion:1,purchasePurpose:'history',sourceRecord:true}]});
  expect(source).toHaveBeenLastCalledWith(account,account.assignments[0],{kind,...(id?{id}:{})});
 }
 expect(account.assignments[0].modules).toEqual(['finance']);expect(ports.select).not.toHaveBeenCalled();
 for(const [audience,path]of [['finance','finance/documents'],['finance','finance/documents/expenses/new'],['people','people-organization/people/directory'],['people','people-organization/people/payroll/new']]as const){
  source.mockClear();const input=productWorkspaceTarget(audience,'c2',`/holdings/acme/${path}`)!;
  await expect(service.read(audience,input)).rejects.toMatchObject({status:403});expect(source).not.toHaveBeenCalled();
 }
 for(const path of ['finance/documents/expenses/new','finance/documents/expenses/e-1/pay','people-organization/people/payroll/runs/r-1/approve','people-organization/people/payroll/runs/%2Fprivate'])expect(peopleFinancialEntry(path)).toBeNull();
});

it('keeps financial source entry closed to missing owner proof, scope, module ceiling, purchase, releases and stale SSO role choices',async()=>{
 const account=access();account.assignments[0]={...account.assignments[0],role:'custom_financial_settlement',modules:['finance']};
 let permissions=['module.finance.read','payroll.payment.read','finance.pay'],modules=['finance'],beta=false,owner=true,released=true;
 const source=vi.fn(async()=>owner?{revision:'private-payroll-image',purchasePurpose:'history' as const}:null),chosen=vi.fn();
 let selection:ReturnType<typeof selected>|null=selected();
 const ports={access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',modules]]),presentationModulesByAssignment:new Map([['assignment',modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:['people','people-payroll'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'draining' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:financial-source'})),history:[]}),ready:async()=>released,peopleFinancialEntry:source,selection:async()=>selection,select:chosen,now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)};
 const service=createProductWorkspace(ports),input=productWorkspaceTarget('people','c2','/holdings/acme/people-organization/people/payroll/runs/original')!;
 expect((await service.read('people',input)).target).toBe(input.target);
 for(const change of [()=>{owner=false;},()=>{released=false;},()=>{beta=true;},()=>{modules=[];},()=>{permissions=['module.finance.read','finance.pay'];}]){
  change();await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});owner=true;released=true;beta=false;modules=['finance'];permissions=['module.finance.read','payroll.payment.read','finance.pay'];
 }
 for(const scope of [{kind:'branch' as const,branchId:'b',branchSlug:'branch',branchName:'Branch'},{kind:'department' as const,branchId:null,departmentId:'d',departmentSlug:'department',departmentName:'Department'}]){
  account.scope={...access().scope,...scope};source.mockClear();await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(source).not.toHaveBeenCalled();
 }
 account.scope=access().scope;account.scope.companyId='foreign';await expect(service.read('people',input)).rejects.toMatchObject({code:'COMPANY_REQUIRED'});account.scope=access().scope;
 account.expiresAt=Date.now()-1;await expect(service.read('people',input)).rejects.toMatchObject({code:'SESSION_EXPIRED'});account.expiresAt=Date.now()+60000;
 selection={...selected(),assignment_id:'another-live-role'};expect((await service.read('people',input)).target).toBeNull();expect(chosen).not.toHaveBeenCalled();
 await expect(service.choose('people',input,'assignment',selection.revision,2)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect(chosen).not.toHaveBeenCalled();
 account.assignments[0].version=2;await expect(service.choose('people',input,'assignment',selection.revision,1)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});await service.choose('people',input,'assignment',selection.revision,2);expect(chosen).toHaveBeenCalledWith(account,expect.objectContaining({assignmentVersion:2,sourceRecord:true}),selection.revision);
});


async function financialOwnerPage(name:'Expenses'|'PayrollPage',ports:Record<string,unknown>){
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript');
 const file=name==='Expenses'?"packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/expenses/screen.tsx":"packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/people/payroll/screen.tsx";
 const source=await readFile(new URL('../../../'+file,import.meta.url),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),node=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text===name);
 if(!node||!ts.isFunctionDeclaration(node))throw Error('ACTUAL_FINANCIAL_PAGE_REQUIRED');
 const declaration=node.getText(tree).replace(/^export\s+default\s+/,''),code=ts.transpileModule(declaration,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,jsxFactory:'createElement',jsxFragmentFactory:'Fragment'}}).outputText;
 return new Function(...Object.keys(ports),`${code}\nreturn ${name};`)(...Object.values(ports)) as (props:{params:Promise<{holdingSlug:string}>;searchParams?:Promise<object>})=>Promise<import('react').ReactElement>;
}
it('renders the actual Expense collection without the unavailable New claim handoff only for the named financial source',async()=>{
 const React=await import('react'),{renderToStaticMarkup}=await import('react-dom/server'),{createElement}=React;
 let financial=false;const source=vi.fn(async()=>financial),actor={...access(),companyId:'c2',holdingSlug:'acme'};
 const wrap=({children}:{children?:import('react').ReactNode})=>createElement('section',{},children),link=({children,href}:{children?:import('react').ReactNode;href?:string})=>createElement('a',{href},children);
 const ports={createElement,Fragment:React.Fragment,requireWorkContext:async()=>actor,publicScope:()=>({kind:'holding',holdingSlug:'acme'}),workspaceBase:()=>'/holdings/acme',listExpenses:async()=>[],peopleFinancialSourcePresentation:source,ExpenseEnvironment:wrap,FeaturePage:wrap,FinanceRecordWorkspace:({children,actions}:{children?:import('react').ReactNode;actions?:import('react').ReactNode})=>createElement('main',{},actions,children),FinanceLegacyHeading:wrap,PageHeader:({title}:{title:string})=>createElement('h1',{},title),FeatureTabs:()=>null,WidgetGrid:wrap,FilterBar:wrap,Field:wrap,TextInput:()=>null,Select:wrap,Button:wrap,FeatureWidget:wrap,EmptyState:({title}:{title:string})=>createElement('p',{},title),UiActionLink:link,ExpenseNewWork:wrap,canonicalUiHref:(href:string)=>href,Link:link,MoneyAmount:()=>null,StatusBadge:()=>null};
 const page=await financialOwnerPage('Expenses',ports),props={params:Promise.resolve({holdingSlug:'acme'}),searchParams:Promise.resolve({})};
 expect(renderToStaticMarkup(await page(props))).toContain('New expense claim');financial=true;const html=renderToStaticMarkup(await page(props));expect(html).not.toContain('New expense claim');expect(html).toContain('Employee expenses');expect(html).toContain('No matching claims');expect(source).toHaveBeenLastCalledWith(actor,'expense');
});
it('retains actual Payroll payment presentation but omits preparation only for the named financial source',async()=>{
 const React=await import('react'),{renderToStaticMarkup}=await import('react-dom/server'),{createElement}=React;
 let financial=false;const source=vi.fn(async()=>financial),actor={...access(),companyId:'c2',holdingSlug:'acme'},seen:Array<Record<string,unknown>>=[];
 const wrap=({children}:{children?:import('react').ReactNode})=>createElement('section',{},children),ports={createElement,Fragment:React.Fragment,requireWorkContext:async()=>actor,peopleFinancialSourcePresentation:source,payrollPurchaseControls:async()=>({newWork:true}),hasPermission:(_actor:unknown,permission:string)=>permission==='payroll.prepare',payrollSummary:async()=>[],listPayrollRuns:async()=>[],payrollRunsPage:async()=>({items:[]}),payrollReadiness:async()=>null,workspaceBase:()=>'/holdings/acme',publicScope:()=>({kind:'holding',holdingSlug:'acme'}),PayrollEnvironment:wrap,PageContent:wrap,PayrollDashboard:(props:Record<string,unknown>)=>{seen.push(props);return createElement('section',{},props.canPrepare?'Prepare payroll':'Payment runs');},Card:wrap,TableFrame:wrap,Table:wrap,RecordLink:wrap,minorToDecimal:()=>'',canonicalUiHref:(href:string)=>href,pageError:(error:unknown)=>{throw error;}};
 const page=await financialOwnerPage('PayrollPage',ports),props={params:Promise.resolve({holdingSlug:'acme'})};expect(renderToStaticMarkup(await page(props))).toContain('Prepare payroll');financial=true;expect(renderToStaticMarkup(await page(props))).toContain('Payment runs');expect(seen.at(-1)).toMatchObject({canPrepare:false,redacted:true});expect(source).toHaveBeenLastCalledWith(actor,'payroll');
});

it('selects financial presentation from the actual canonical request and current selected owner entry, preserving ordinary and unexpected failures',async()=>{
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),{peopleFinancialEntry}=await import('@erpx/server-people_payroll/public/rules/financial-product-entry'),{workspacePathParts}=await import('@erpx/feature-work_projection/lib/destination-registry'),actualHosts=await vi.importActual<typeof import("@erpx/feature-runtime/lib/product-hosts")>("@erpx/feature-runtime/lib/product-hosts");
 const file='packages/server-people_payroll/src/adapters/financial-product-presentation-request.ts',source=await readFile(new URL('../../../'+file,import.meta.url),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true),node=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='peopleFinancialSourcePresentation');
 if(!node||!ts.isFunctionDeclaration(node))throw Error('ACTUAL_FINANCIAL_PRESENTATION_REQUIRED');
 const code=ts.transpileModule(node.getText(tree).replace(/^export\s+/,''),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 let request=new Headers({host:'console-dev-erpx.jesadakorn.com','x-erpx-page-path':'/holdings/acme/finance/documents/expenses'});
 const entry=vi.fn(async()=>({sourceRecord:true})),ports={headers:async()=>request,productHostForHost:(host:string|null)=>actualHosts.productHostForHost(host,{routing:true,sso:true,environment:'development'}),workspacePathParts,peopleFinancialEntry,productPageAccess:entry};
 const presentation=new Function(...Object.keys(ports),`${code}\nreturn peopleFinancialSourcePresentation;`)(...Object.values(ports)) as (actor:{companyId:string},kind:'expense'|'payroll')=>Promise<boolean>;
 expect(await presentation({companyId:'c2'},'expense')).toBe(false);expect(entry).not.toHaveBeenCalled();
 request.set('host','finance-dev-erpx.jesadakorn.com');expect(await presentation({companyId:'c2'},'expense')).toBe(true);expect(entry).toHaveBeenLastCalledWith('finance','c2','/holdings/acme/finance/documents/expenses');
 entry.mockResolvedValue({sourceRecord:false});expect(await presentation({companyId:'c2'},'expense')).toBe(false);
 request.set('x-erpx-page-path','/holdings/acme/finance/documents/expenses/new');entry.mockClear();expect(await presentation({companyId:'c2'},'expense')).toBe(false);expect(entry).not.toHaveBeenCalled();
 request=new Headers({host:'people-dev-erpx.jesadakorn.com','x-erpx-page-path':'/holdings/acme/people-organization/people/payroll/dashboard'});entry.mockResolvedValue({sourceRecord:true});expect(await presentation({companyId:'c2'},'payroll')).toBe(true);
 const error=new Error('Actual owner database failed');entry.mockRejectedValue(error);await expect(presentation({companyId:'c2'},'payroll')).rejects.toBe(error);
});

// Current source-port decisions are mocked here; these tests do not replace native IAM/source proof.
import {payrollPostingPrerequisitesEntry,payrollPostingPrerequisitesLanding,payrollPostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/public/rules/payroll-posting-prerequisites-entry';
import {purchasedCapabilities as payrollPrerequisiteCapabilities} from '@erpx/server-commercial/public/rules/packages';
import type {CompanyPackageGrant as PayrollPrerequisiteGrant,CompanyPackageHistoryRight as PayrollPrerequisiteHistory} from '@erpx/server-commercial/public/packages';
it('opens only exact Payroll posting prerequisites for a Finance-approve role and keeps retained history and explicit role selection separate',async()=>{
 const account=access();account.expiresAt=Date.now()+60000;account.assignments[0]={...account.assignments[0],role:'custom_finance_approver',modules:['finance']};
 let history=false,selection={...selected(),assignment_id:'another-live-role'};const grants=():PayrollPrerequisiteGrant[]=>['people','people-payroll'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active',startsAt:history?Date.now()+60000:0,endsAt:null,version:history?3:1,reason:'Fixture',evidence:'test:posting-entry'})),retained=():PayrollPrerequisiteHistory[]=>history?['people','people-payroll'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,sourceGrantVersion:1,sourceEventId:'private-old-'+packageId,retainedAt:0})):[];
 const owner=vi.fn(async()=>({revision:'private-payroll-prerequisite-revision',purchasePurpose:'history' as const})),choose=vi.fn(async()=>{selection=selected();return selection;}),ports={access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',['finance.approve']]]),beta:false}),purchaseFacts:async()=>({grants:grants(),history:retained()}),ready:async()=>true,payrollPostingPrerequisitesEntry:owner,selection:async()=>selection,select:choose,now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)},service=createProductWorkspace(ports),target='/holdings/acme/'+payrollPostingPrerequisitesLanding+'?payGroupId=actual-group&tag=one&tag=two#posting',input=productWorkspaceTarget('people','c2',target)!;
 const pending=await service.read('people',input);expect(pending.target).toBeNull();expect(pending.options).toMatchObject([{assignmentId:'assignment',assignmentVersion:1,role:'custom_finance_approver',target,purchasePurpose:'history',sourceRecord:true}]);expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0]);expect(JSON.stringify(pending.options)).not.toMatch(/private-payroll|permissions|session|revision/);expect(choose).not.toHaveBeenCalled();
 await expect(service.choose('people',input,'assignment',selection.revision,2)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});await expect(service.choose('people',input,'assignment',selection.revision+1,1)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect(choose).not.toHaveBeenCalled();await service.choose('people',input,'assignment',selection.revision,1);expect(choose).toHaveBeenCalledExactlyOnceWith(account,expect.objectContaining({assignmentId:'assignment',assignmentVersion:1,target,sourceRecord:true,purchasePurpose:'history'}),3);expect(await service.read('people',input)).toMatchObject({target,sourceRecord:true});
 history=true;expect(payrollPrerequisiteCapabilities(grants(),'c2','history',Date.now(),retained())).toContain('people.payroll');expect(payrollPrerequisiteCapabilities(grants(),'c2','new-work',Date.now(),retained())).not.toContain('people.payroll');expect(await service.read('people',input)).toMatchObject({target,sourceRecord:true,options:[{purchasePurpose:'history'}]});expect(choose).toHaveBeenCalledTimes(1);expect(account.assignments[0]).toMatchObject({role:'custom_finance_approver',modules:['finance']});
 for(const path of ['people-organization/people/payroll/settings','people-organization/people/payroll/settings/posting/extra','people-organization/people/payroll/settings/posting/new','people-organization/people/directory/current']){owner.mockClear();const adjacent=productWorkspaceTarget('people','c2','/holdings/acme/'+path);if(adjacent)await expect(service.read('people',adjacent)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});else expect(adjacent).toBeNull();expect(owner).not.toHaveBeenCalled();}
});
it('denies Payroll prerequisite entry before the owner port when module, scope, Beta, purchases, releases or current role facts are missing',async()=>{
 const account=access();account.expiresAt=Date.now()+60000;account.assignments[0]={...account.assignments[0],role:'custom_finance_approver',modules:['finance']};let modules=['finance'],permissions=['finance.approve'],beta=false,unreleased='',eligible=true,packages=['people','people-payroll'];
 const owner=vi.fn(async()=>eligible?{revision:'private-payroll-prerequisite-source',purchasePurpose:'history' as const}:null),select=vi.fn(),ports={access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',modules]]),presentationModulesByAssignment:new Map([['assignment',modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:packages.map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:posting-entry'})),history:[]}),ready:async(_actor:unknown,moduleId:string)=>moduleId!==unreleased,payrollPostingPrerequisitesEntry:owner,selection:async()=>selected(),select,now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)},service=createProductWorkspace(ports),input=productWorkspaceTarget('people','c2','/holdings/acme/'+payrollPostingPrerequisitesLanding)!;
 for(const change of [()=>{modules=[];},()=>{modules=['people_payroll'];},()=>{permissions=[];},()=>{permissions=['finance.read','finance.write'];},()=>{permissions=['payroll.prepare','payroll.read'];},()=>{beta=true;},()=>{packages=['people'];},()=>{packages=['people-payroll'];},()=>{unreleased='people_payroll';},()=>{unreleased='finance';},()=>{account.assignments[0].version=0;}]){change();owner.mockClear();await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();modules=['finance'];permissions=['finance.approve'];beta=false;packages=['people','people-payroll'];unreleased='';account.assignments[0].version=1;}
 eligible=false;await expect(service.read('people',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;
 for(const [scope,url]of [[{kind:'branch' as const,branchId:'b',branchSlug:'main',branchName:'Main'},'/holdings/acme/branches/main/'],[{kind:'department' as const,branchId:null,departmentId:'d',departmentSlug:'team',departmentName:'Team'},'/holdings/acme/departments/team/']]as const){account.scope={...access().scope,...scope};owner.mockClear();await expect(service.read('people',productWorkspaceTarget('people','c2',url+payrollPostingPrerequisitesLanding)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}
 account.scope=access().scope;account.scope.companyId='foreign';await expect(service.read('people',input)).rejects.toMatchObject({code:'COMPANY_REQUIRED'});account.scope=access().scope;account.expiresAt=Date.now()-1;await expect(service.read('people',input)).rejects.toMatchObject({code:'SESSION_EXPIRED'});expect(select).not.toHaveBeenCalled();
 expect(payrollPostingPrerequisitesEntry(payrollPostingPrerequisitesLanding)).toBe(true);expect(payrollPostingPrerequisitesPresentationAllowed({scopeKind:'holding',modules:['finance'],permissions:['finance.approve']})).toBe(true);expect(payrollPostingPrerequisitesPresentationAllowed({scopeKind:'holding',modules:['people_payroll'],permissions:['finance.approve']})).toBe(false);
});

// Closed Expense setup source-port integration only; no native owner/source proof is inferred.
import {expensePostingPrerequisitesEntry,expensePostingPrerequisitesLanding,expensePostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/public/rules/expense-posting-prerequisites-entry';
it('opens only exact Expense setup for current Company Finance read/write or read/approve while preserving explicit selection and retained history',async()=>{
 const account=access();account.expiresAt=Date.now()+60000;account.assignments[0]={...account.assignments[0],role:'custom_expense_finance',modules:['finance']};let permissions=['finance.read','finance.write'],history=false,selection={...selected(),assignment_id:'another-live-role'};
 const grants=():PayrollPrerequisiteGrant[]=>['people','people-expenses'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active',startsAt:history?Date.now()+60000:0,endsAt:null,version:history?3:1,reason:'Fixture',evidence:'test:expense-setup-entry'})),retained=():PayrollPrerequisiteHistory[]=>history?['people','people-expenses'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,sourceGrantVersion:1,sourceEventId:'private-old-'+packageId,retainedAt:0})):[],owner=vi.fn(async()=>({revision:'private-expense-prerequisite-revision',purchasePurpose:'history' as const})),choose=vi.fn(async()=>{selection=selected();return selection;}),ports={access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:grants(),history:retained()}),ready:async()=>true,expensePostingPrerequisitesEntry:owner,selection:async()=>selection,select:choose,now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)},service=createProductWorkspace(ports),target='/holdings/acme/'+expensePostingPrerequisitesLanding+'?tag=one&tag=two#references',input=productWorkspaceTarget('finance','c2',target)!;
 expect(await service.read('finance',input)).toMatchObject({target:null,options:[{assignmentId:'assignment',assignmentVersion:1,role:'custom_expense_finance',target,sourceRecord:true,purchasePurpose:'history'}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0]);expect(choose).not.toHaveBeenCalled();await expect(service.choose('finance',input,'assignment',selection.revision,2)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});await expect(service.choose('finance',input,'assignment',selection.revision+1,1)).rejects.toMatchObject({code:'WORK_CONTEXT_CHANGED'});expect(choose).not.toHaveBeenCalled();await service.choose('finance',input,'assignment',selection.revision,1);expect(choose).toHaveBeenCalledExactlyOnceWith(account,expect.objectContaining({assignmentId:'assignment',assignmentVersion:1,target,purchasePurpose:'history',sourceRecord:true}),3);
 permissions=['finance.read','finance.approve'];expect(await service.read('finance',input)).toMatchObject({target,sourceRecord:true});history=true;expect(payrollPrerequisiteCapabilities(grants(),'c2','history',Date.now(),retained())).toContain('people.expenses');expect(payrollPrerequisiteCapabilities(grants(),'c2','new-work',Date.now(),retained())).not.toContain('people.expenses');const result=await service.read('finance',input);expect(result).toMatchObject({target,sourceRecord:true,options:[{purchasePurpose:'history'}]});expect(JSON.stringify(result.options)).not.toMatch(/private-expense|permissions|session|revision/);expect(choose).toHaveBeenCalledTimes(1);expect(account.assignments[0]).toMatchObject({role:'custom_expense_finance',modules:['finance']});
 for(const path of ['finance/setup','finance/setup/expenses/new','finance/setup/expenses/charts','finance/setup/expenses/extra','finance/accounting/journals','finance/documents','people-organization/people/directory/current']){owner.mockClear();const adjacent=productWorkspaceTarget('finance','c2','/holdings/acme/'+path);if(adjacent)await expect(service.read('finance',adjacent)).rejects.toMatchObject({status:403});else expect(adjacent).toBeNull();expect(owner).not.toHaveBeenCalled();}
});
it('denies Expense setup source entry before its owner port for missing Finance ceiling/current permission/purchase/release or non-Company scope',async()=>{
 const account=access();account.expiresAt=Date.now()+60000;account.assignments[0]={...account.assignments[0],role:'custom_expense_finance',modules:['finance']};let modules=['finance'],permissions=['finance.read','finance.write'],beta=false,unreleased='',eligible=true,packages=['people','people-expenses'];const owner=vi.fn(async()=>eligible?{revision:'private-expense-current-source',purchasePurpose:'history' as const}:null),select=vi.fn(),ports={access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',modules]]),presentationModulesByAssignment:new Map([['assignment',modules]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta}),purchaseFacts:async()=>({grants:packages.map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:expense-setup-entry'})),history:[]}),ready:async(_actor:unknown,moduleId:string)=>moduleId!==unreleased,expensePostingPrerequisitesEntry:owner,selection:async()=>selected(),select,now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)},service=createProductWorkspace(ports),input=productWorkspaceTarget('finance','c2','/holdings/acme/'+expensePostingPrerequisitesLanding)!;
 for(const change of [()=>{modules=[];},()=>{modules=['people_payroll'];},()=>{permissions=[];},()=>{permissions=['finance.read'];},()=>{permissions=['finance.write'];},()=>{permissions=['finance.approve'];},()=>{permissions=['expense.read','expense.approve'];},()=>{beta=true;},()=>{packages=['people'];},()=>{packages=['people-expenses'];},()=>{unreleased='people_payroll';},()=>{unreleased='finance';},()=>{account.assignments[0].version=0;}]){change();owner.mockClear();await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();modules=['finance'];permissions=['finance.read','finance.write'];beta=false;packages=['people','people-expenses'];unreleased='';account.assignments[0].version=1;}
 eligible=false;await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});eligible=true;for(const [scope,url]of [[{kind:'branch' as const,branchId:'b',branchSlug:'main',branchName:'Main'},'/holdings/acme/branches/main/'],[{kind:'department' as const,branchId:null,departmentId:'d',departmentSlug:'team',departmentName:'Team'},'/holdings/acme/departments/team/']]as const){account.scope={...access().scope,...scope};owner.mockClear();await expect(service.read('finance',productWorkspaceTarget('finance','c2',url+expensePostingPrerequisitesLanding)!)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});expect(owner).not.toHaveBeenCalled();}account.scope=access().scope;account.scope.companyId='foreign';await expect(service.read('finance',input)).rejects.toMatchObject({code:'COMPANY_REQUIRED'});account.scope=access().scope;account.expiresAt=Date.now()-1;await expect(service.read('finance',input)).rejects.toMatchObject({code:'SESSION_EXPIRED'});expect(select).not.toHaveBeenCalled();expect(expensePostingPrerequisitesEntry(expensePostingPrerequisitesLanding)).toBe(true);expect(expensePostingPrerequisitesPresentationAllowed({scopeKind:'holding',modules:['finance'],permissions:['finance.read','finance.approve']})).toBe(true);
});

// Existing owner authority only: no module, permission, purchase or record-reader grant.
it.each(['expense.prepare','expense.approve'])('retains original %s Expense accounting entry for an explicitly chosen Finance-only role without requiring Treasury',async permission=>{
 const account=access();account.expiresAt=Date.now()+60000;account.assignments[0]={...account.assignments[0],role:'custom_expense_accounting',modules:['finance']};let selection={...selected(),assignment_id:'other-live-role'},allowed=true;const permissions=['module.finance.read','finance.read','expense.read',permission],owner=vi.fn(async()=>allowed?{revision:'private-accounting-source',purchasePurpose:'history' as const}:null),select=vi.fn(async()=>{selection=selected();return selection;}),ports={access:async()=>account,modules:async()=>({modulesByAssignment:new Map([['assignment',['finance']]]),presentationModulesByAssignment:new Map([['assignment',['finance']]]),permissionsByAssignment:new Map([['assignment',permissions]]),beta:false}),purchaseFacts:async()=>({grants:['people','people-expenses'].map(packageId=>({companyId:'c2',packageId,packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:1,reason:'Fixture',evidence:'test:original-expense-accounting-entry'})),history:[]}),ready:async()=>true,peopleFinancialEntry:owner,selection:async()=>selection,select,now:Date.now,error:(status:number,code:string,message:string)=>new ApiError(status,code,message)},service=createProductWorkspace(ports),target='/holdings/acme/finance/documents/expenses/actual-claim/overview?tag=one&tag=two#accounting',input=productWorkspaceTarget('finance','c2',target)!;
 expect(await service.read('finance',input)).toMatchObject({target:null,options:[{assignmentId:'assignment',assignmentVersion:1,target,purchasePurpose:'history',sourceRecord:true}]});expect(owner).toHaveBeenLastCalledWith(account,account.assignments[0],{kind:'expense',id:'actual-claim'});expect(select).not.toHaveBeenCalled();await service.choose('finance',input,'assignment',selection.revision,1);expect(await service.read('finance',input)).toMatchObject({target,sourceRecord:true});expect(select).toHaveBeenCalledTimes(1);expect(permissions).not.toContain('expense.pay');expect(permissions).not.toContain('expense.audit.read');expect(account.assignments[0].modules).toEqual(['finance']);allowed=false;await expect(service.read('finance',input)).rejects.toMatchObject({code:'PRODUCT_ROLE_REQUIRED'});for(const path of ['finance/documents','finance/accounting/journals','finance/documents/expenses/new','people-organization/people/directory/current']){owner.mockClear();const other=productWorkspaceTarget('finance','c2','/holdings/acme/'+path);if(other)await expect(service.read('finance',other)).rejects.toMatchObject({status:403});expect(owner).not.toHaveBeenCalled();}
});
it('preserves the exact original Expense Treasury branch and requires both original reader and accounting duty on the new accounting branch',async()=>{
 const {peopleFinancialPresentationAllowed:allowed}=await import('@erpx/server-people_payroll/public/rules/financial-product-entry'),context={scopeKind:'holding',beta:false,modules:['finance']};expect(allowed({...context,permissions:['expense.pay']},'expense')).toBe(true);for(const action of ['expense.prepare','expense.approve'])expect(allowed({...context,permissions:['expense.read',action]},'expense')).toBe(true);for(const permissions of [[],['expense.read'],['expense.prepare'],['expense.approve'],['expense.review.read'],['expense.audit.read'],['finance.read','finance.write'],['finance.read','finance.approve']])expect(allowed({...context,permissions},'expense')).toBe(false);for(const patch of [{modules:[]},{modules:['people_payroll']},{scopeKind:'branch'},{scopeKind:'department'},{beta:true}])expect(allowed({...context,permissions:['expense.read','expense.prepare'],...patch},'expense')).toBe(false);expect(allowed({...context,permissions:['expense.read','expense.approve']},'payroll')).toBe(false);expect(allowed({...context,permissions:['finance.pay','payroll.payment.read']},'payroll')).toBe(true);
});
