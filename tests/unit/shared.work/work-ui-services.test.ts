import {expect,it,vi} from "vitest";
import {createFrontAppService,type FrontPorts} from "../../../src/server/modules/work_projection/application/front-app";
import {createFrontQuickAccessService} from "../../../src/server/modules/work_projection/application/front-quick-access";
import {createListPageReader} from "../../../src/server/modules/work_projection/application/list-page";
import {createStaffApiService,type StaffApiPorts} from "../../../src/server/modules/work_projection/application/staff-api";
import {createStaffPageService} from "../../../src/server/modules/work_projection/application/staff-page";
import {createNavigationContextService} from "../../../src/server/modules/work_projection/application/navigation-context";
import {createNavigationCountService} from "../../../src/server/modules/work_projection/application/navigation-counts";
import {createStorefrontIconService} from "../../../src/server/modules/work_projection/application/front-icon";
import {createPageLayoutService,type PageLayoutUnit} from "../../../src/server/modules/work_projection/application/page-layouts";
import {createFlowSettingsHubQuery,type FlowSettingsHubPorts} from "../../../src/server/modules/work_projection/application/flow-settings-hub";
import {createRequestAdministrationQuery,type RequestAdministrationPorts} from "../../../src/server/modules/records/application/request-administration";
import {createSocialPageQuery,type SocialPagePorts} from "../../../src/server/modules/collaboration/application/social-page";
import {createBusinessDocumentBrandQuery,type BusinessDocumentBrandPorts} from "../../../src/server/modules/brand/application/business-document";
import type {WorkContext,AccountAccessContext} from "../../../src/server/modules/identity_access/public/authorization";
import type {ActiveWorkSelection} from "../../../src/server/modules/work_projection/domain/work-role-store";
import type {WorkspaceHoldingChoice} from "../../../src/lib/work_projection/workspace-choice";
import {ApiError} from "../../../src/server/modules/runtime/public/http";
function only<T>(values:Partial<T>):T {return new Proxy(values,{get(target,name){if(name==="then")return undefined;if(name in target)return Reflect.get(target,name);throw Error(`Unexpected port ${String(name)}`);}}) as T;}
const actor={userId:"user",sessionId:"session",companyId:"company",holdingId:"holding",holdingSlug:"acme",role:"owner",assignmentId:"assignment",scope:{kind:"holding",holdingSlug:"acme"}} as unknown as WorkContext;
const error=(status:number,code:string,message:string)=>new ApiError(status,code,message);
const assignment={id:"assignment",role:"owner",companyId:"company",branchId:null,modules:[],scopeKind:"holding",scopeNodeId:"company",version:2} as AccountAccessContext["assignments"][number];
const access={sessionId:"session",expiresAt:2000,userId:"user",name:"User",email:"user@test.invalid",scope:{kind:"holding",holdingId:"holding",holdingSlug:"acme",companyId:"company"},assignments:[assignment],requestHeaders:new Headers()} as AccountAccessContext;
const selection:ActiveWorkSelection={session_id:"session",user_id:"user",company_id:"company",assignment_id:"assignment",assignment_version:2,revision:1,expires_at:1500};
function front(overrides:Partial<FrontPorts>={}){return createFrontAppService(only<FrontPorts>({error,isApiError:(value):value is ApiError=>value instanceof ApiError,now:()=>1000,prepareSlug:async()=>({id:"holding",slug:"acme"}),prepareHolding:async()=>{},accountAccess:async()=>access,roles:async()=>new Map([["owner",{permissions:["pos.read"]}]]),selection:async()=>selection,...overrides}));}
it("requires the selected role's current version and expiry for Front",async()=>{
 expect((await front().frontRoleState("acme","company","pos","holding")).selected?.id).toBe("assignment");
 for(const changed of [{...selection,expires_at:1000},{...selection,assignment_version:1}])expect((await front({selection:async()=>changed}).frontRoleState("acme","company","pos","holding")).selected).toBeUndefined();
});
it("rejects company mismatch and unsupported department assignments before selection",async()=>{
 await expect(front({accountAccess:async()=>({...access,scope:{...access.scope,companyId:"other"}})}).ensureFrontRole("acme","company","pos","holding")).rejects.toMatchObject({code:"FRONT_NOT_FOUND"});
 await expect(front({accountAccess:async()=>({...access,scope:{...access.scope,kind:"department",branchId:null,departmentId:"department",departmentSlug:"operations",departmentName:"Operations"}})}).ensureFrontRole("acme","company","pos","holding")).rejects.toMatchObject({code:"FRONT_ACCESS_DENIED"});
});
it("selects an eligible live assignment without changing remembered preferences",async()=>{
 let selected=false;const select=vi.fn(async()=>{selected=true;return selection;});
 const service=front({selection:async()=>selected?selection:null,select});await service.ensureFrontRole("acme","company","pos","holding");
 expect(select).toHaveBeenCalledWith(expect.objectContaining({assignmentVersion:2,revision:0,rememberPreference:false,expiresAt:2000}));
});
it("recovers a concurrent Front selection only after rereading current authority",async()=>{
 let selected=false;const service=front({selection:async()=>selected?selection:null,select:async()=>{selected=true;throw error(409,"WORK_CONTEXT_CHANGED","changed");}});
 expect((await service.ensureFrontRole("acme","company","pos","holding")).selected?.id).toBe("assignment");
 await expect(front({selection:async()=>null,select:async()=>{throw error(409,"WORK_CONTEXT_CHANGED","changed");}}).ensureFrontRole("acme","company","pos","holding")).rejects.toMatchObject({code:"WORK_CONTEXT_CHANGED"});
});
it("does not hide unexpected selection failures",async()=>{const failure=Error("connection refused");await expect(front({selection:async()=>null,select:async()=>{throw failure;}}).ensureFrontRole("acme","company","pos","holding")).rejects.toBe(failure);});
it("uses active owner lineage facts and rejects malformed Front locations",async()=>{
 const branchDepartment=vi.fn(async()=>({slug:"ops",branch_slug:"bkk"}));const service=front({facts:only<FrontPorts["facts"]>({branchDepartment})});
 expect(await service.frontScopeFromLocation("acme","company","branches/bkk/departments/ops")).toEqual({kind:"department",holdingSlug:"acme",branchSlug:"bkk",departmentSlug:"ops"});expect(branchDepartment).toHaveBeenCalledWith("company","ops","bkk");
 await expect(service.frontScopeFromLocation("acme","company","branches/bkk/extra")).rejects.toMatchObject({code:"WORKSPACE_NOT_FOUND"});
});
const company={id:"company",slug:"hotel",display_name:"Hotel",short_name:"  " ,accent_color:null,logo_attachment_id:"private-original",logo_publication_id:null};
it("does not fetch private original logo bytes without an approved publication",async()=>{
 const file=vi.fn();const service=front({facts:only<FrontPorts["facts"]>({companyBrand:async()=>company}),publishedBrand:async()=>null,file});
 expect(await service.storefrontLogo("acme","hotel")).toBeNull();expect(file).not.toHaveBeenCalled();expect(await service.frontCompanyBrand("acme","hotel")).toMatchObject({shortName:"Hotel",accentColor:"#3659c9",logoAttachmentId:"private-original"});
});
it("rejects unavailable or revoked Front app roles in company choices",async()=>{
 const choice={kind:"holding",holdingSlug:"acme",companies:[{id:"company",label:"Hotel"}]} as WorkspaceHoldingChoice;
 const service=createFrontQuickAccessService({workspaces:async()=>[choice],activeCompanies:async()=>[{id:"company",slug:"hotel"}],frontCompanyApp:async()=>null});expect(await service.frontCompanyChoices("pos")).toEqual([]);
});
it("renders icon decisions before delegating to the image provider",async()=>{
 const png=vi.fn<(svg:string)=>Promise<Uint8Array<ArrayBuffer>>>(async()=>new Uint8Array([1]));const service=createStorefrontIconService({base64:()=>"encoded",png});await service.storefrontAppIconPng({shortName:"&Hotel",accentColor:"invalid",app:"pms",size:100});expect(png.mock.calls[0][0]).toContain("&amp;");expect(png.mock.calls[0][0]).toContain("#3659c9");expect(png.mock.calls[0][0]).toContain("PMS");
});
const row=(id:string,key=id)=>({id,page_key:key});
function pages(rows:ReturnType<typeof row>[]){const read=vi.fn<(request:{descending:boolean;limit:number})=>Promise<typeof rows>>(async()=>rows);const service=createListPageReader({hash:value=>value,decode:JSON.parse,encode:JSON.stringify,invalidCursor:()=>error(422,"INVALID_CURSOR","restart"),rows:read});return {read,service};}
it("enforces cursor scope before issuing a repository read",async()=>{const {service,read}=pages([]);await expect(service({scope:"current",cursor:JSON.stringify({key:"k",id:"i",reverse:false,scope:"other"})})).rejects.toMatchObject({code:"INVALID_CURSOR"});expect(read).not.toHaveBeenCalled();});
it("creates scoped forward and reverse cursors across duplicate keys",async()=>{
 const {service,read}=pages([row("a","same"),row("b","same"),row("c","same")]);const first=await service({scope:"company",limit:2});expect(first.items.map(i=>i.id)).toEqual(["a","b"]);expect(first.previousCursor).toBeNull();expect(JSON.parse(first.nextCursor!)).toMatchObject({key:"same",id:"b",reverse:false});
 const back=await service({scope:"company",limit:2,cursor:JSON.stringify({...JSON.parse(first.nextCursor!),reverse:true})});expect(back.items.map(i=>i.id)).toEqual(["b","a"]);expect(read.mock.calls[1][0]).toMatchObject({descending:true,limit:3});
});
it("clamps safe page sizes and rejects oversized cursors without decoding",async()=>{const {service,read}=pages([]);await service({scope:null,limit:500});expect(read.mock.calls[0][0]).toMatchObject({limit:101});await expect(service({scope:null,cursor:"a".repeat(4097)})).rejects.toMatchObject({code:"INVALID_CURSOR"});expect(read).toHaveBeenCalledTimes(1);});
it("retains the staff projection's expected absence while exposing unexpected failures",async()=>{
 const base={actor:async()=>actor,prepare:async()=>{},settings:async()=>({levels:[]}),projects:async()=>[],base:()=>"/holdings/acme",expectedAbsence:(e:unknown)=>e instanceof ApiError};
 const expected=createStaffPageService({...base,staff:async()=>{throw error(404,"MISSING","staff absent");}});expect((await expected("acme")).staff).toBeNull();const failure=Error("database unavailable");await expect(createStaffPageService({...base,staff:async()=>{throw failure;}})("acme")).rejects.toBe(failure);
});
it("does not load unavailable application catalogue in a non-production navigation projection",async()=>{
 const availableApps=vi.fn();const service=createNavigationContextService({administration:async()=>({sections:[],manageOrganization:false,managePeople:false,owner:false,manageAiProviders:false}),prepare:async()=>{},base:()=>"/holdings/acme",production:false,availableApps,financeV2:()=>true,executiveVisible:async()=>false});expect(await service(actor)).toMatchObject({financeWorkspaceV2:true,executiveVisible:false});expect(availableApps).not.toHaveBeenCalled();
});
it("keeps Home read cursor scope and injected time consistent with the feed count",async()=>{const mark=vi.fn(async()=>{}),unread=vi.fn(async()=>3);const service=createNavigationCountService({now:()=>123,base:()=>"scope-base",mark,unread});await service.markHomeRead(actor);expect(mark).toHaveBeenCalledWith(actor,"scope-base",123);expect(await service.unreadHomeCount(actor,124)).toBe(3);expect(unread).toHaveBeenCalledWith(actor,"scope-base",124);});
function staff(overrides:Partial<StaffApiPorts>={}){return createStaffApiService(only<StaffApiPorts>({requireWorkContext:async()=>actor,prepare:async()=>{},now:()=>123,error,requestPublicOrigin:()=>"https://app.test.invalid",...overrides}));}
function request(path:string,method="GET",headers:Record<string,string>={}){return {request:new Request("https://app.test.invalid/api/"+path,{method,headers,...(method==="GET"?{}:{body:"{}"})}),context:{params:Promise.resolve({holdingSlug:"acme",path:path.split("/")})}};}
it("rejects mutation origin and missing receipt key before dispatching owner commands",async()=>{for(const [headers,code] of [[{origin:"https://evil.invalid","Idempotency-Key":"key"},"ORIGIN_FORBIDDEN"],[{},"COMMAND_KEY_REQUIRED"]] as const){const r=request("notifications/n/read","POST",headers);await expect(staff()("staff",r.request,r.context)).rejects.toMatchObject({code});}});
it("marks only the current user's company notification through the owner port",async()=>{const notificationRead=vi.fn(async()=>{}),r=request("notifications/n/read","POST",{"Idempotency-Key":"key"});expect(await staff({facts:only<StaffApiPorts["facts"]>({notificationRead})})("staff",r.request,r.context)).toEqual({id:"n"});expect(notificationRead).toHaveBeenCalledWith(123,"n","company","user");});
it("guards approval queue projection with live company authority",async()=>{const r=request("pending");await expect(staff({companyAuthority:async()=>false})("approvals",r.request,r.context)).rejects.toMatchObject({code:"NOT_FOUND"});});
it("loads project ownership before projecting its members and tasks",async()=>{const order:string[]=[];const r=request("project");const service=staff({getProject:async()=>{order.push("project-authority");return {} as Awaited<ReturnType<StaffApiPorts["getProject"]>>;},facts:only<StaffApiPorts["facts"]>({projectMembers:async()=>{order.push("members");return {results:[]};}}),listTasks:async()=>{order.push("tasks");return [] as unknown as Awaited<ReturnType<StaffApiPorts["listTasks"]>>;}});await service("projects",r.request,r.context);expect(order).toEqual(["project-authority","members","tasks"]);});
it("keeps layout receipt replay inside the transaction and filters now-unavailable widgets",async()=>{
 const snapshot={personal:{schemaVersion:1,widgets:[{id:"current-profiles",columns:6,height:320,hidden:false},{id:"removed-widget",columns:6,height:320,hidden:false}]},organization:null,version:1,organizationVersion:0,canPublish:true};
 type LayoutPorts=Parameters<typeof createPageLayoutService<string>>[0];
 const transactionMock=vi.fn(async(action:(tx:PageLayoutUnit<string>)=>Promise<unknown>)=>action(only<PageLayoutUnit<string>>({lock:async()=>{},workGuards:()=>[],iamGuards:()=>[],clearAssertions:()=>"clear",batch:async()=>{},receipt:async()=>({request_hash:"hash",result_json:JSON.stringify(snapshot)})})));
 const transaction=transactionMock as LayoutPorts["transaction"];
 const session={session:{user:{id:"user"},session:{id:"session"}}} as unknown as Awaited<ReturnType<LayoutPorts["currentSession"]>>;
 const service=createPageLayoutService<string>(only<LayoutPorts>({error,now:()=>123,hash:()=>"hash",currentSession:async()=>session,requireWorkContext:async()=>({...actor,role:"holding_owner"}),navigation:async()=>({workspaceBase:"/holdings/acme",scopeKind:"holding",role:"holding_owner",adminSections:["people","organization"]}),base:()=>"/holdings/acme",prepare:async()=>{},unit:()=>only<PageLayoutUnit<string>>({companyAuthority:async()=>false}),transaction}));
 const replay=await service("POST",async()=>({key:"key",input:{action:"save",expectedVersion:0,layout:{schemaVersion:1,widgets:[]}}}),"people-directory","acme");expect(replay).toMatchObject({canPublish:false,version:1});expect(replay.personal?.widgets.map(widget=>widget.id)).toEqual(["current-profiles"]);expect(transactionMock).toHaveBeenCalledOnce();
});

function flowSettingsPorts(overrides: Partial<FlowSettingsHubPorts> = {}) {
  return only<FlowSettingsHubPorts>({
    prepare: async () => undefined,
    access: async () => ({ sections: [], managePeople: false, manageOrganization: false, owner: false, manageAiProviders: false }),
    authority: async () => ({ departments: [], seats: [], history: [], policies: [], canAppointKinds: [], canPublishDepartmentIds: [] }),
    canManageLeave: async () => false,
    companyName: async () => null,
    ...overrides,
  });
}
it("keeps inaccessible Flow settings histories and readiness repositories unread", async () => {
  const result = await createFlowSettingsHubQuery(flowSettingsPorts())("history");
  expect(result).toMatchObject({ policyHistory: [], leavePolicies: [], configurationHistory: [], serviceOptions: null, canPublish: false, companyName: "Current Company" });
});
it("uses only visible department ids and preserves Flow history category order", async () => {
  const departments = vi.fn<FlowSettingsHubPorts["departmentHistory"]>(async () => []);
  const entry = (kind: string) => ({ title: kind, kind, version: 2, actor: "reviewer", at: 42 });
  const app = createFlowSettingsHubQuery(flowSettingsPorts({
    access: async () => ({ sections: [], managePeople: true, manageOrganization: true, owner: true, manageAiProviders: false }),
    authority: async () => ({ departments: [{ id: "visible-department", display_name: "Visible" }], seats: [], history: [], policies: [], canAppointKinds: [], canPublishDepartmentIds: ["visible-department"] }),
    departmentHistory: departments, canManageLeave: async () => true,
    serviceOptions: async () => only<Awaited<ReturnType<FlowSettingsHubPorts["serviceOptions"]>>>({ teams: [] }),
    leavePolicies: async () => [{ name: "Annual", version: 2, status: "active" }],
    leaveHistory: async () => [entry("Leave policy")], serviceHistory: async () => [entry("Service coverage")], spendingHistory: async () => [entry("Spending rules revision")],
    companyName: async () => "Scoped Company",
  }));
  const result = await app("history");
  expect(departments).toHaveBeenCalledExactlyOnceWith(["visible-department"]);
  expect(result.configurationHistory.map(row => row.kind)).toEqual(["Leave policy", "Service coverage", "Spending rules revision"]);
  expect(result.companyName).toBe("Scoped Company"); expect(result.canPublish).toBe(true);
});
it("does not request configuration history for the Flow templates tab", async () => {
  const app = createFlowSettingsHubQuery(flowSettingsPorts({
    access: async () => ({ sections: [], managePeople: false, manageOrganization: true, owner: false, manageAiProviders: false }),
  }));
  expect((await app("templates")).configurationHistory).toEqual([]);
});
it("prepares the Flow repository before evaluating current authority", async () => {
  const calls: string[] = [];
  const ports = flowSettingsPorts();
  await createFlowSettingsHubQuery({ ...ports, prepare: async () => { calls.push("prepare"); }, access: async () => { calls.push("access"); return ports.access(); }, authority: async () => { calls.push("authority"); return ports.authority(); } })("templates");
  expect(calls).toEqual(["prepare", "access", "authority"]);
});
it("denies request administration before reading request or route data", async () => {
  const app = createRequestAdministrationQuery(only<RequestAdministrationPorts>({ prepare: async () => undefined, owner: async () => false }));
  expect(await app("private-request")).toBeNull(); expect(await app()).toBeNull();
});
it("keeps single-post Home queries separate from feeds and personal schedule", async () => {
  const post = only<Awaited<ReturnType<SocialPagePorts["post"]>>>({ id: "post" });
  const result = await createSocialPageQuery(only<SocialPagePorts>({ prepare: async () => undefined, post: async id => { expect(id).toBe("post"); return post; }, audience: async () => only<Awaited<ReturnType<SocialPagePorts["audience"]>>>({ branches: [], audiences: [], projects: [] }), moderate: async () => false }))({ beta: false }, { postId: "post" });
  expect(result.posts).toHaveLength(1); expect(result.posts[0]).toBe(post); expect(result.post).toBe(post); expect(result.milestones).toEqual([]); expect(result.initialDay).toBeUndefined();
});
it("recovers Home personal-summary failure without hiding either feed stream", async () => {
  const feed = vi.fn<SocialPagePorts["feed"]>(async () => []);
  const result = await createSocialPageQuery(only<SocialPagePorts>({ prepare: async () => undefined, seed: async () => { throw Error("Schedule unavailable"); }, feed, audience: async () => only<Awaited<ReturnType<SocialPagePorts["audience"]>>>({ branches: [], audiences: [], projects: [] }), moderate: async () => true }))({ beta: false }, { tag: "team" });
  expect(await result.initialDay).toBeNull();
  expect(feed.mock.calls.map(([filter]) => filter)).toEqual([{ limit: 8, stream: "conversations", tag: "team" }, { limit: 8, stream: "milestones", tag: "team" }]);
  expect(result.moderate).toBe(true);
});
it("resolves the document's recorded Branch before applying requested print mode", async () => {
  const document = vi.fn<BusinessDocumentBrandPorts["document"]>(async () => only<Awaited<ReturnType<BusinessDocumentBrandPorts["document"]>>>({ mode: "both" }));
  const recordBranch = vi.fn<BusinessDocumentBrandPorts["recordBranch"]>(async () => "recorded-branch");
  const app = createBusinessDocumentBrandQuery({ prepare: async () => undefined, recordBranch, document });
  await app({ kind: "invoice", recordId: "invoice", branchId: "ambient-branch", requestedMode: "both" });
  expect(recordBranch).toHaveBeenCalledExactlyOnceWith("invoice", "invoice");
  expect(document).toHaveBeenLastCalledWith({ branchId: "recorded-branch", requestedMode: "both" });
  await app({ branchId: "provided-branch" });
  expect(recordBranch).toHaveBeenCalledTimes(1); expect(document).toHaveBeenLastCalledWith({ branchId: "provided-branch", requestedMode: undefined });
});
it("filters inactive request routing and loads resource candidates only for blocked equipment work", async () => {
  type Request = Awaited<ReturnType<RequestAdministrationPorts["list"]>>[number];
  type Detail = Awaited<ReturnType<RequestAdministrationPorts["detail"]>>;
  const requests = ([
    { id: "draft", kind: "equipment", state: "draft" },
    { id: "blocked-equipment", kind: "equipment", state: "awaiting_approval" },
    { id: "blocked-service", kind: "service", state: "received" },
    { id: "healthy-service", kind: "service", state: "received" },
  ] satisfies Pick<Request, "id" | "kind" | "state">[]).map(request => only<Request>(request));
  const detail = vi.fn<RequestAdministrationPorts["detail"]>(async id => only<Detail>({ request: requests.find(request => request.id === id)!, blockedReason: id.startsWith("blocked-") ? "Coverage unavailable" : null }));
  const route = vi.fn<RequestAdministrationPorts["route"]>(async () => ({ scope: { company_id: "company", branch_id: null, department_id: null }, steps: [], blockedReason: null }));
  const issues = await createRequestAdministrationQuery({ prepare: async () => undefined, owner: async () => true, list: async () => requests, detail, route })();
  expect(detail.mock.calls.map(([id]) => id)).toEqual(["blocked-equipment", "blocked-service", "healthy-service"]);
  expect(issues?.map(issue => issue.request.id)).toEqual(["blocked-equipment", "blocked-service"]);
  expect(route).toHaveBeenCalledTimes(1); expect(route.mock.calls[0][0]).toBe(requests[1]); expect(issues?.[1].route).toBeNull();
});
it("retains a specifically requested healthy routing record instead of requiring a blocker", async () => {
  type Detail = Awaited<ReturnType<RequestAdministrationPorts["detail"]>>;
  const request = only<Detail["request"]>({ id: "selected-service", kind: "service", state: "received" });
  const detail = vi.fn<RequestAdministrationPorts["detail"]>(async () => only<Detail>({ request, blockedReason: null }));
  const issues = await createRequestAdministrationQuery(only<RequestAdministrationPorts>({ prepare: async () => undefined, owner: async () => true, detail }))("selected-service");
  expect(detail).toHaveBeenNthCalledWith(1, "selected-service"); expect(detail).toHaveBeenNthCalledWith(2, "selected-service");
  expect(issues?.[0].request).toBe(request); expect(issues?.[0].route).toBeNull();
});
