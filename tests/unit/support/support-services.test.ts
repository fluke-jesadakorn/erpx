import { describe, expect, it, vi } from "vitest";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { createValidateServiceScope } from "@erpx/server-support/application/service-access";
import { createServiceTeamHandlers, createResolveServiceTeam } from "@erpx/server-support/application/service-teams";
import { createServiceTeamOptions } from "@erpx/server-support/application/service-team-options";
import { type ServiceTeam } from "@erpx/server-support/domain/service-teams";
const actor = { userId: "owner", companyId: "company", holdingId: "holding", branchId: null, departmentId: null, role: "holding_owner" } as WorkContext;
const scope = { company_id: "company", branch_id: "branch", department_id: "department" };
const team: ServiceTeam = { id: "team", company_id: "company", name: "Local team", active: 1, version: 4, configuration_json: "{}" };
describe("Support service team policy through typed ports", () => {
    it("rejects out-of-workspace locations before querying memberships", async () => { const member = vi.fn(async () => true); await expect(createValidateServiceScope({ error: (violation) => Object.assign(new Error(violation.message), violation), member, company: async () => true, branch: async () => true, department: async () => true })({ ...actor, branchId: "another" }, scope)).rejects.toMatchObject({ code: "REQUEST_SCOPE_FORBIDDEN" }); expect(member).not.toHaveBeenCalled(); });
    it("deduplicates current person principals before current-scope eligibility", async () => { const member = vi.fn(async () => true), handlers = await createServiceTeamHandlers({ members: async () => [{ user_id: "old", responsibility: "member", name: "Old" }, { user_id: "current", responsibility: "lead", name: "Current" }], principals: async () => new Map([["old", "current"]]), member })(scope, "team"); expect(handlers).toEqual([{ user_id: "current", responsibility: "lead", name: "Current" }]); expect(member).toHaveBeenCalledTimes(1); });
    it("keeps an unavailable local team as a blocker instead of falling back", async () => { const handlers = vi.fn(async () => []); expect(await createResolveServiceTeam({ teams: async () => [{ ...team, scope_kind: "department", scope_node_id: "department" }, { ...team, id: "fallback", scope_kind: "holding", scope_node_id: "company" }], handlers })(scope, "it")).toBeNull(); expect(handlers).toHaveBeenCalledTimes(1); expect(handlers).toHaveBeenCalledWith(scope, "team"); });
    it("uses retained coverage for inactive teams without reading live coverage", async () => { const coverage = vi.fn(async () => []); const data = await createServiceTeamOptions({ owner: async () => true, nodes: async () => [], teams: async () => [{ ...team, active: 0, configuration_json: JSON.stringify({ name: "Support team", active: false, expectedVersion: 4, members: [{ userId: "lead", responsibility: "lead" }], coverage: [{ category: "it", scopeKind: "branch", scopeNodeId: "branch" }] }) }], members: async () => [], history: async () => [], handlers: async () => [], coverage })(actor); expect(data.teams[0].coverage).toEqual([{ category: "it", scope_kind: "branch", scope_node_id: "branch" }]); expect(coverage).not.toHaveBeenCalled(); });
});
import { createGetSupportTicket, createSupportWorkspace } from "@erpx/server-support/application/support";
import { createSupportEvidenceGuard, createSupportEvidence } from "@erpx/server-support/application/support-evidence";
import { createSupportSessionContext } from "@erpx/server-support/application/support-sessions";
import { supportSessionProjection, type SupportSession } from "@erpx/server-support/domain/support-sessions";
import type { SupportCommandPorts, SupportCommand, SupportMutation, SupportRow } from "@erpx/server-support/application/command";
const mutation: SupportMutation = { native: { guard: "current" } };
const ticket: SupportRow = { id: "ticket", status: "open", source_kind: "sales_order", created_by: actor.userId, owner_id: actor.userId, owner_eligible: 1, branch_id: "branch", department_id: null, version: 4 };
const policy = { calendar: { timezone: "Asia/Bangkok", weekdays: [1, 2, 3, 4, 5], opensAtMinute: 540, closesAtMinute: 1020, holidays: [] }, targetMinutes: 60, pauseWhileWaiting: true };
function runtime() { return { authorize: vi.fn(), error: (status: number, code: string, message: string) => Object.assign(new Error(message), { status, code }), now: vi.fn(() => Date.parse("2026-09-21T02:15:00Z")), replay: vi.fn<SupportCommandPorts["replay"]>(async () => null), commit: vi.fn(async (command: SupportCommand) => command.result) }; }
const request = { ticketId: "ticket", ticketVersion: 4, requestedFor: actor.userId, expiresAt: Date.parse("2026-09-21T03:00:00Z"), reason: "Temporary customer investigation" };
const session: SupportSession = { id: "session", status: "pending", requested_for: "colleague", requested_for_name: "Colleague", created_by: "colleague", reviewed_by: null, version: 1, expires_at: request.expiresAt, updated_at: 100, title: "Access review", can_review: 1, target_eligible: 1 };
describe("Support ticket lifecycle through semantic ports", () => {
    it("rejects an inaccessible source after the scoped ticket query", async () => { const ports = { ...runtime(), ticket: async () => ticket, sourceVisible: async () => false }; await expect(createGetSupportTicket(ports)(actor, "ticket")).rejects.toMatchObject({ status: 404, code: "SUPPORT_TICKET_NOT_FOUND" }); });
    it("filters source visibility sequentially and keeps order creation capability", async () => { const visible = vi.fn(async (_a: WorkContext, r: SupportRow) => r.id === "visible"); const data = await createSupportWorkspace({ ...runtime(), tickets: async () => [{ ...ticket, id: "hidden" }, { ...ticket, id: "visible" }], sourceVisible: visible, orderSources: async () => [{ id: "order" }] })(actor); expect(data.items).toEqual([{ ...ticket, id: "visible" }]); expect(data.canCreateOrder).toBe(true); expect(visible.mock.calls.map(c => c[1].id)).toEqual(["hidden", "visible"]); });
});
describe("Support evidence ownership", () => {
    it("keeps Service-owned lifecycle and evidence changes out of Helpdesk while retaining readable evidence", async () => {
        const managed = { ...ticket, source_kind: "field_service" };
        const guard = vi.fn(async () => mutation), eligible = vi.fn(async () => true);
        await expect(createSupportEvidenceGuard({ ...runtime(), ticket: async () => managed, eligible, guard })(actor, "ticket", 4, "key")).rejects.toMatchObject({ code: "SOURCE_WORKSPACE_REQUIRED" });
        expect(guard).not.toHaveBeenCalled(); expect(eligible).not.toHaveBeenCalled();
        expect(await createSupportEvidence({ ticket: async () => managed, items: async () => [{ id: "retained" }], eligible, readOnly: () => false })(actor, "ticket")).toEqual({ items: [{ id: "retained" }], canChange: false, canShare: false });
    });
    it("freezes evidence as soon as resolution is proposed", async () => { const guard = vi.fn(async () => mutation); await expect(createSupportEvidenceGuard({ ...runtime(), ticket: async () => ({ ...ticket, status: "resolved" }), eligible: async () => true, guard })(actor, "ticket", 4, "key")).rejects.toMatchObject({ code: "SUPPORT_EVIDENCE_LOCKED" }); expect(guard).not.toHaveBeenCalled(); });
    it("checks current evidence eligibility before capturing transaction guards", async () => { const guard = vi.fn(async () => mutation); await expect(createSupportEvidenceGuard({ ...runtime(), ticket: async () => ticket, eligible: async () => false, guard })(actor, "ticket", 4, "key")).rejects.toMatchObject({ code: "SUPPORT_EVIDENCE_ASSIGNMENT" }); expect(guard).not.toHaveBeenCalled(); });
    it("makes retained evidence read-only when rollback mode is active", async () => { expect(await createSupportEvidence({ ticket: async () => ticket, items: async () => [{ id: "file" }], eligible: async () => true, readOnly: () => true })(actor, "ticket")).toEqual({ items: [{ id: "file" }], canChange: false, canShare: false }); });
});
describe("Temporary Support capability lifecycle", () => {
    it("retains expired access as completed and unavailable for management", () => { const view = supportSessionProjection(actor, [{ ...session, status: "approved", expires_at: 100 }], () => 200)[0]; expect(view).toMatchObject({ effective_status: "expired", canManage: false, canReview: false }); });
    it("expires pending review at its exact deadline and never offers an approval afterwards", () => {
        const clock = vi.fn(() => 100), pending = { ...session, status: "pending", created_by: "requester", requested_for: "colleague", can_review: 1, expires_at: 100 };
        const view = supportSessionProjection(actor, [pending], clock)[0];
        expect(view).toMatchObject({ effective_status: "expired", canReview: false, canApprove: false, canManage: false, canIssueCapability: false });
        expect(clock).toHaveBeenCalledOnce();
    });
    it("requires an active context grant before projecting any attachment facts", async () => { const evidence = vi.fn(async () => [{ id: "private-file" }]); await expect(createSupportSessionContext({ ...runtime(), context: async () => null, evidence })(actor, "session")).rejects.toMatchObject({ code: "SUPPORT_CAPABILITY_REQUIRED" }); expect(evidence).not.toHaveBeenCalled(); });
});
import { createSupportTicketSourceGuards } from "@erpx/server-support/application/support";
describe("Support source transaction composition", () => {
    it("retains current-ticket guards without reading an absent origin", async () => { const source = vi.fn(async () => { throw new Error("unused"); }), origin = vi.fn(async () => mutation), combine = vi.fn(() => mutation); expect(await createSupportTicketSourceGuards({ ticket: async () => ticket, current: async () => mutation, source, origin, combine })(actor, "key", "ticket", 4, "handoff")).toBe(mutation); expect(source).not.toHaveBeenCalled(); expect(combine).not.toHaveBeenCalled(); });
    it("checks source-origin availability after the current ticket before composing guards", async () => { const calls: string[] = [], current = vi.fn(async () => { calls.push("current"); return mutation; }), source = vi.fn(async () => { calls.push("origin"); throw new Error("source is unavailable"); }), combine = vi.fn(() => mutation); await expect(createSupportTicketSourceGuards({ ticket: async () => ({ ...ticket, source_record_json: JSON.stringify({ kind: "asset", id: "asset" }) }), current, source, origin: async () => mutation, combine })(actor, "key", "ticket", 4, "handoff")).rejects.toThrow("source is unavailable"); expect(calls).toEqual(["current", "origin"]); expect(combine).not.toHaveBeenCalled(); });
});


// New Support-only Customer Portal callers. Original owner/controller functions
// run with explicit request ports; real React SSR and native Response/workFetch.
// These bounded checks do not claim a mounted browser or native IAM acceptance.
import {afterEach} from 'vitest';
import * as React from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
import {createSupportPortalCommands,readSupportPortal,validSupportPortalWorkspace,type SupportPortalSource} from '@erpx/feature-support/lib/ticket-portal-command';
import {workFetch} from '@erpx/feature-work_projection/lib/work-fetch';
import {setContextSwitching} from '@erpx/feature-work_projection/lib/context-switch';
import {SupportTicketPortalSetup,supportPortalRequest} from '@erpx/feature-support/components/ticket-portal';
import {supportTicketPortalPrepareSchema,supportTicketPortalDecisionSchema,supportTicketPortalResponseSchema,supportTicketPortalSelectionSchema,type SupportTicketPortalWorkspace} from '@erpx/server-support/domain/ticket-portal';
const portalBase='/holdings/acme',portalContext='a'.repeat(64),portalPrepare={ticketId:'ticket',ticketVersion:4,customerName:'Actual recipient',email:'recipient@example.test',subject:'Approved public subject',summary:'Only the approved customer-facing summary',expiresAt:1900000000000,reason:'Explicit customer access review'};
function portalView(id='binding'):SupportTicketPortalWorkspace{return{companyId:'company',schemaAvailable:true,available:true,contextKey:portalContext,canPrepare:true,ticket:{id:'ticket',version:4,title:'Internal case title',status:'open',sourceKind:'general'},bindings:[{id,ticketId:'ticket',ticketVersion:4,customerName:portalPrepare.customerName,email:portalPrepare.email,subject:portalPrepare.subject,summary:portalPrepare.summary,expiresAt:portalPrepare.expiresAt,status:'draft',version:1,canApprove:false,canReject:false,canRevoke:false,canRespond:false,response:null,acknowledgedAt:null}]};}
function supportPortalBrowser(){const node={isConnected:true,dataset:{workUser:'manager',workRevision:'1',workCompany:'company',workAssignment:'support-role',workStale:'false'}},surface=Object.assign(new EventTarget(),{location:{origin:'https://helpdesk.example.test'}}),events:CustomEvent[]=[];let boundary:typeof node|null=node,edit='original-edit';const form={isConnected:true,getAttribute:()=>edit} as unknown as HTMLFormElement;for(const name of ['erpx:record-command-saved','erpx:work-changed'])surface.addEventListener(name,event=>events.push(event as CustomEvent));vi.stubGlobal('window',surface);vi.stubGlobal('document',{documentElement:{dataset:{}},querySelector:(selector:string)=>selector==='[data-work-revision]'?boundary:null,activeElement:{closest:()=>form}});setContextSwitching(false);const source:SupportPortalSource={base:portalBase,companyId:'company',contextKey:portalContext,ticketId:'ticket',ticketVersion:4,bindingId:null,bindingVersion:null,responseVersion:0};return{node,events,source,form,newer:()=>{edit='newer-edit';},remove:()=>{boundary=null;},restore:()=>{boundary=node;}};}
afterEach(()=>{if(typeof document!=='undefined')setContextSwitching(false);vi.unstubAllGlobals();});
function portalTransport(calls:{method:string;body:BodyInit|null|undefined;key:string|null}[],post?:(key:string)=>Response){return vi.fn<(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async(_url,init)=>{const method=init?.method??'GET',key=new Headers(init?.headers).get('Idempotency-Key');calls.push({method,body:init?.body,key});return method==='POST'?(post?.(key!)??Response.json({data:{id:`support-portal-${key}`,status:'draft',version:1}})):Response.json({data:portalView(`support-portal-${calls.find(call=>call.method==='POST')!.key}`)});});}
it('the actual Support form builds only the selected native ticket and explicit public fields for the real strict owner schemas',()=>{const fields={customerName:portalPrepare.customerName,email:'RECIPIENT@example.test',subject:portalPrepare.subject,summary:portalPrepare.summary,reason:portalPrepare.reason,ticketId:'forged-ticket',ticketVersion:'9',expiresLocal:'Displayed local time'},request=supportPortalRequest('prepare',fields,portalView(),undefined,String(portalPrepare.expiresAt));expect(request.kind).toBe('prepare');expect(supportTicketPortalPrepareSchema.parse(request.body)).toEqual(portalPrepare);expect(()=>supportPortalRequest('prepare',{},portalView(),undefined,'not-an-instant')).toThrow('valid access expiry');expect(supportTicketPortalPrepareSchema.safeParse({...request.body,privateNotes:'not permitted'}).success).toBe(false);});
it('the actual approval and publication payloads capture their owner editions without approving or closing the staff ticket',()=>{const view=portalView();view.bindings[0].version=3;view.bindings[0].response={id:'reply',version:2,message:'Prior public response',publishedAt:100};expect(supportPortalRequest('approve',{reason:'Independent actual review'},view,'binding')).toEqual({kind:'decision',bindingId:'binding',body:supportTicketPortalDecisionSchema.parse({expectedVersion:3,action:'approve',reason:'Independent actual review'})});expect(supportPortalRequest('response',{message:'Explicit shared response',reason:'Independent actual publication'},view,'binding')).toEqual({kind:'response',bindingId:'binding',body:supportTicketPortalResponseSchema.parse({expectedBindingVersion:3,expectedResponseVersion:2,message:'Explicit shared response',reason:'Independent actual publication'})});});
it('real Support setup SSR keeps binding reviews and authorized-case handoff while making no initial GET or private-summary copy',()=>{const fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);const view=portalView(),html=renderToStaticMarkup(React.createElement(SupportTicketPortalSetup,{base:portalBase,initial:view,bindingId:'binding'}));expect(html).toContain('Choose a Helpdesk case');expect(html).toContain('/customers-sales/customers/service/portal/binding');expect(html).toContain('Approved public subject');expect(html).not.toContain('Approve customer ticket access</button>');expect(html).not.toContain('Publish a customer response</button>');expect(html).not.toContain('value="Internal case title"');expect(fetcher).not.toHaveBeenCalled();});
it('real Support SSR follows action-specific current owner flags without discovering unavailable actions',()=>{const view=portalView();view.bindings[0].canReject=true;const html=renderToStaticMarkup(React.createElement(SupportTicketPortalSetup,{base:portalBase,initial:view,bindingId:'binding'}));expect(html).toContain('Reject customer ticket access</button>');expect(html).not.toContain('Approve customer ticket access</button>');expect(html).not.toContain('Publish a customer response</button>');});
it.each(['role','stale','removed boundary','generation','source'] as const)('real staff command waits for original native JSON and retains body/key after late %s',async change=>{const c=supportPortalBrowser(),commands=createSupportPortalCommands(()=>c.source,workFetch),calls:{method:string;body:BodyInit|null|undefined;key:string|null}[]=[];let start!:()=>void,release!:()=>void;const reading=new Promise<void>(resolve=>{start=resolve;}),ready=new Promise<void>(resolve=>{release=resolve;});class OriginalBody extends Response{override async json(){start();await ready;return super.json();}}vi.stubGlobal('fetch',portalTransport(calls,key=>calls.filter(value=>value.method==='POST').length===1?new OriginalBody(JSON.stringify({data:{id:`support-portal-${key}`,status:'draft',version:1}})):Response.json({data:{id:`support-portal-${key}`,status:'draft',version:1}})));const request=commands.submit({kind:'prepare',body:portalPrepare},{form:c.form,formEditVersion:'original-edit'});const rejected=expect(request).rejects.toThrow();try{await reading;expect(c.events).toEqual([]);if(change==='role')c.node.dataset.workRevision='2';else if(change==='stale')c.node.dataset.workStale='true';else if(change==='removed boundary')c.remove();else if(change==='generation'){setContextSwitching(true);setContextSwitching(false);}else c.source.contextKey='b'.repeat(64);release();await rejected;}finally{release();}expect(commands.pending()).toBe(true);expect(calls).toHaveLength(1);expect(c.events).toEqual([]);c.node.dataset.workRevision='1';c.node.dataset.workStale='false';c.restore();c.source.contextKey=portalContext;c.newer();const outcome=await commands.retry();expect(outcome.unchanged()).toBe(false);expect(outcome.acknowledge()).toBe(true);expect(outcome.acknowledge()).toBe(false);expect(calls[1]).toEqual(calls[0]);expect(calls[2].method).toBe('GET');expect(c.events.map(event=>event.type)).toEqual(['erpx:record-command-saved','erpx:work-changed']);expect(c.events[0].detail).toEqual({form:c.form,formEditVersion:'original-edit'});expect(commands.pending()).toBe(false);});
it('a valid staff receipt followed by an unreadable current binding retains its original request through a later 4xx and exact recovery',async()=>{const c=supportPortalBrowser(),commands=createSupportPortalCommands(()=>c.source),calls:{key:string|null;body:BodyInit|null|undefined}[]=[];let posts=0,gets=0;vi.stubGlobal('fetch',async(_url:RequestInfo|URL,init?:RequestInit)=>{if(init?.method==='POST'){const key=new Headers(init.headers).get('Idempotency-Key');calls.push({key,body:init.body});return ++posts===2?Response.json({error:{message:'Later authority refused'}},{status:403}):Response.json({data:{id:`support-portal-${key}`,status:'draft',version:1}});}if(!gets++)throw Error('Current owner GET failed');return Response.json({data:portalView(`support-portal-${calls[0].key}`)});});await expect(commands.submit({kind:'prepare',body:portalPrepare})).rejects.toThrow('Current owner GET failed');expect(commands.applied()).toBe(true);await expect(commands.retry()).rejects.toThrow('Later authority refused');expect(commands.pending()).toBe(true);expect(c.events).toEqual([]);const outcome=await commands.retry();expect(outcome.acknowledge()).toBe(true);expect(calls[2]).toEqual(calls[0]);expect(c.events.map(value=>value.type)).toEqual(['erpx:work-changed']);});
it('malformed/foreign native receipts keep the staff key and reject newer commands; a fresh definitive rejection is releasable',async()=>{const c=supportPortalBrowser(),commands=createSupportPortalCommands(()=>c.source),calls:{method:string;body:BodyInit|null|undefined;key:string|null}[]=[];vi.stubGlobal('fetch',portalTransport(calls,()=>Response.json({data:{id:'another-binding',status:'draft',version:1}})));await expect(commands.submit({kind:'prepare',body:portalPrepare})).rejects.toThrow('incomplete');expect(commands.pending()).toBe(true);expect(()=>commands.submit({kind:'prepare',body:{...portalPrepare,summary:'Newer unsent summary'}})).toThrow('newer draft is retained');expect(c.events).toEqual([]);const fresh=createSupportPortalCommands(()=>c.source);vi.stubGlobal('fetch',async()=>Response.json({error:{message:'Reviewed rejection'}},{status:422}));await expect(fresh.submit({kind:'prepare',body:portalPrepare})).rejects.toThrow('Reviewed rejection');expect(fresh.pending()).toBe(false);});
it('initial stale work refuses a staff POST and no-data workspace decoding refuses malformed/foreign owners',async()=>{const c=supportPortalBrowser(),fetcher=vi.fn();vi.stubGlobal('fetch',fetcher);c.node.dataset.workStale='true';const commands=createSupportPortalCommands(()=>c.source);await expect(commands.submit({kind:'prepare',body:portalPrepare})).rejects.toThrow('account, Company or role');expect(fetcher).not.toHaveBeenCalled();expect(commands.pending()).toBe(false);expect(validSupportPortalWorkspace(portalView(),'another-company')).toBe(false);expect(validSupportPortalWorkspace({...portalView(),bindings:[{...portalView().bindings[0],version:'1'}]},'company')).toBe(false);expect(validSupportPortalWorkspace({...portalView(),available:false},'company')).toBe(false);});
it('staff current-owner reads recheck after delayed original JSON before publishing current bindings',async()=>{const c=supportPortalBrowser();let start!:()=>void,release!:()=>void;const reading=new Promise<void>(resolve=>{start=resolve;}),ready=new Promise<void>(resolve=>{release=resolve;});class OriginalBody extends Response{override async json(){start();await ready;return super.json();}}vi.stubGlobal('fetch',async()=>new OriginalBody(JSON.stringify({data:portalView()})));const read=readSupportPortal(()=>c.source,{bindingId:'binding'}),rejected=expect(read).rejects.toThrow();try{await reading;c.source.companyId='other';release();await rejected;}finally{release();}expect(c.events).toEqual([]);});
async function supportRoute(file:string,ports:Record<string,unknown>){const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL('../../../'+file,import.meta.url),'utf8'),tree=ts.createSourceFile(file,source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),body=tree.statements.filter(node=>!ts.isImportDeclaration(node)).map(node=>node.getText(tree)).join('\n'),emitted=ts.transpileModule(body,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText,exports:Record<string,unknown>={};new Function('exports',...Object.keys(ports),emitted)(exports,...Object.values(ports));return exports;}
function nativeSupportPagePorts(data:unknown,evidence:unknown={items:[]}){vi.stubEnv('ERPX_NATIVE_SUPPORT_READ_CONFIG','test-support-read');class TestApiError extends Error{readonly status:number;readonly code:string;constructor(status:number,code:string,message:string){super(message);this.status=status;this.code=code;}}return{ApiError:TestApiError,headers:async()=>new Headers(),createNativeSupportReader:()=>({detail:async()=>data,evidence:async()=>evidence})};}
afterEach(()=>vi.unstubAllEnvs());
it('the actual closed staff route delegates each strict owner command and keeps unsupported selectors, duplicate source and Origin/key failures out',async()=>{const {ApiError,errorResponse}=await import('@erpx/server-runtime/public/http'),{boundedBody}=await import('@erpx/server-legal/public/legal/body'),read=vi.fn(async()=>portalView()),prepare=vi.fn(async()=>({id:'support-portal-command',status:'draft',version:1})),decide=vi.fn(async()=>({id:'binding',status:'approved',version:2})),respond=vi.fn(async()=>({id:'reply',status:'published',version:1})),route=await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/ticket-portal/[[...path]]/route.ts",{ApiError,errorResponse,boundedBody,requestPublicOrigin:(request:Request)=>new URL(request.url).origin,requireWorkContext:async()=>actor,supportTicketPortalWorkspace:read,prepareSupportTicketPortal:prepare,decideSupportTicketPortal:decide,publishSupportTicketPortalResponse:respond,supportTicketPortalSelectionSchema,supportTicketPortalPrepareSchema,supportTicketPortalDecisionSchema,supportTicketPortalResponseSchema}),get=route.GET as (r:Request,c:{params:Promise<{holdingSlug:string;path?:string[]}>})=>Promise<Response>,post=route.POST as typeof get,ctx=(path:string[]=[])=>({params:Promise.resolve({holdingSlug:'acme',path})}),request=(body:unknown,headers:Record<string,string>={'Origin':'https://helpdesk.test','Idempotency-Key':'command'})=>new Request('https://helpdesk.test/api/owner',{method:'POST',headers,body:JSON.stringify(body)});expect((await get(new Request('https://helpdesk.test/api/owner?ticketId=ticket'),ctx())).status).toBe(200);expect(read).toHaveBeenLastCalledWith(actor,{ticketId:'ticket'});expect((await post(request(portalPrepare),ctx())).status).toBe(201);expect(prepare).toHaveBeenCalledWith(actor,portalPrepare,'command');expect((await post(request({expectedVersion:1,action:'approve',reason:'Actual independent review'}),ctx(['binding','decisions']))).status).toBe(200);expect((await post(request({expectedBindingVersion:2,expectedResponseVersion:0,message:'Actual response',reason:'Actual independent publication'}),ctx(['binding','responses']))).status).toBe(200);expect(decide).toHaveBeenCalledOnce();expect(respond).toHaveBeenCalledOnce();for(const path of [['binding','other'],['binding','responses','extra'],['binding']])expect((await post(request({}),ctx(path))).status).toBe(404);expect((await get(new Request('https://helpdesk.test/api/owner?ticketId=a&ticketId=b'),ctx())).status).toBe(404);expect((await post(request(portalPrepare,{'Origin':'https://foreign.test','Idempotency-Key':'command'}),ctx())).status).toBe(403);expect((await post(request(portalPrepare,{'Origin':'https://helpdesk.test'}),ctx())).status).toBe(422);expect(prepare).toHaveBeenCalledOnce();});
it('dedicated staff pages use only the named owner SSR read and exact empty parallel slots',async()=>{const view=portalView(),read=vi.fn(async()=>view),Component=()=>null,ports={React,requireWorkContext:async()=>actor,supportTicketPortalWorkspace:read,supportTicketPortalSelectionSchema,publicScope:(value:WorkContext)=>value,workspaceBase:()=>portalBase,SupportTicketPortalSetup:Component},root=await supportRoute("packages/feature-support/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/portal/page.tsx",ports),Page=root.default as (p:{params:Promise<{holdingSlug:string}>;searchParams:Promise<Record<string,string>>})=>Promise<React.ReactElement>;await Page({params:Promise.resolve({holdingSlug:'acme'}),searchParams:Promise.resolve({ticketId:'ticket'})});expect(read).toHaveBeenCalledWith(actor,{ticketId:'ticket'});for(const path of ["packages/feature-support/src/routes/holdings/[holdingSlug]/@record/customers-sales/customers/service/portal/page.tsx","packages/feature-support/src/routes/holdings/[holdingSlug]/@record/customers-sales/customers/service/portal/[bindingId]/page.tsx"]){const slot=await supportRoute(path,{});expect((slot.default as ()=>null)()).toBeNull();}});

async function actualSupportPortalSubmit(){const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL("../../../packages/feature-support/src/components/ticket-portal.tsx",import.meta.url),'utf8'),tree=ts.createSourceFile('actual-support-portal.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),component=tree.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='SupportTicketPortalSetup');if(!component||!ts.isFunctionDeclaration(component)||!component.body)throw Error('Original Support owner required');const submit=component.body.statements.find(node=>ts.isFunctionDeclaration(node)&&node.name?.text==='submit');let drawer:import('typescript').JsxElement|undefined;const visit=(node:import('typescript').Node)=>{if(ts.isJsxElement(node)&&node.openingElement.tagName.getText(tree)==='DraftFormDrawer')drawer=node;ts.forEachChild(node,visit);};visit(component);if(!submit||!drawer)throw Error('Original submit/drawer required');const c=supportPortalBrowser(),view=portalView(),commands=createSupportPortalCommands(()=>c.source),state={busy:false,uncertain:false,error:'',notice:null as {owner:string;text:string;bindingId:string}|null,open:true,refreshed:null as unknown},fields:Record<string,string>={customerName:portalPrepare.customerName,email:portalPrepare.email,subject:portalPrepare.subject,summary:portalPrepare.summary,reason:portalPrepare.reason},revision={current:0},setter=(key:keyof typeof state)=>(value:unknown)=>Object.assign(state,{[key]:value}),compile=(text:string)=>ts.transpileModule(text,{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React}}).outputText;
 class SubmittedFormData extends FormData{constructor(target:unknown){super();if(target!==c.form)throw Error('Original form required');this.set('expiresAt',String(portalPrepare.expiresAt));}}
 const identity=(source:SupportPortalSource)=>JSON.stringify([source.base,source.companyId,source.ticketId,source.bindingId]),ports={busyRef:{current:false},setBusy:setter('busy'),setError:setter('error'),setNotice:setter('notice'),revision,owner:identity,current:c.source,mode:'prepare',allowed:true,commands:{current:commands},supportPortalRequest,fields,effective:{base:portalBase,view},FormData:SubmittedFormData,setRefreshed:setter('refreshed'),setUncertain:setter('uncertain'),base:portalBase,initial:view,setFields:(value:Record<string,string>)=>{for(const key of Object.keys(fields))delete fields[key];Object.assign(fields,value);},setOpen:setter('open'),live:{current:c.source}},run=new Function(...Object.keys(ports),compile(`${submit.getText(tree)}\nreturn submit;`))(...Object.values(ports)) as (form:HTMLFormElement,retry?:boolean)=>Promise<void>;
 const passthrough=(props:{children?:React.ReactNode})=>React.createElement('div',{},props.children);
 return{...c,commands,state,fields,revision,run:()=>run(c.form),retry:()=>run(c.form,true),newer(){c.newer();fields.summary='Newer unsent public summary';revision.current++;},drawerHtml(){const saved=state.notice,savedLink=saved?React.createElement('a',{href:`${portalBase}/customers-sales/customers/service/portal/${saved.bindingId}`},'Open saved customer access'):null,renderPorts={React,t:(_thai:string,english:string)=>english,...state,mode:'prepare',titles:{prepare:'Request verified customer ticket access'},dirty:true,changed:false,sameOwner:true,allowed:true,view,binding:undefined,fields,change:()=>{},base:portalBase,bindingId:undefined,form:{current:c.form},submit:run,setOpen:setter('open'),discard:()=>{},setSeed:()=>{},saved,savedLink,permitted:()=>true,DraftFormDrawer:passthrough,InlineNotice:passthrough,SupportTicketPortalFields:passthrough,Button:(props:{children?:React.ReactNode;disabled?:boolean;type?:'submit'|'button'})=>React.createElement('button',{disabled:props.disabled,type:props.type},props.children)},render=new Function(...Object.keys(renderPorts),compile(`return (${drawer!.getText(tree)});`))(...Object.values(renderPorts)) as React.ReactNode;return renderToStaticMarkup(render);}};
}
it('actual staff submit and open drawer recover the original request, retain newer summary and expose the exact saved binding inside the drawer',async()=>{const c=await actualSupportPortalSubmit(),calls:{method:string;body:BodyInit|null|undefined;key:string|null}[]=[];vi.stubGlobal('fetch',portalTransport(calls, key=>{if(calls.filter(call=>call.method==='POST').length===1)throw Error('Original native reply lost');return Response.json({data:{id:`support-portal-${key}`,status:'draft',version:1}});}));await c.run();expect(c.state.uncertain).toBe(true);expect(c.drawerHtml()).toContain('Retry original Portal request');c.newer();await c.retry();expect(calls[1]).toEqual(calls[0]);expect(c.fields.summary).toBe('Newer unsent public summary');expect(c.state.open).toBe(true);expect(c.state.notice?.bindingId).toBe(`support-portal-${calls[0].key}`);expect(c.drawerHtml()).toContain('Open saved customer access');expect(c.drawerHtml()).toContain('Your newer draft remains.');expect(c.events[0].detail.formEditVersion).toBe('original-edit');});

it.each([false,true])('actual original Helpdesk detail preserves the no-addon workflow and displays only its owner-proven optional Portal handoff=%s',async permitted=>{const data={row:{id:'ticket',title:'Original internal case',issue:'Private staff issue',status:'open',version:4,source_kind:'general',owner_eligible:1,reviewer_count:1},canManage:true,managedInFieldService:false,canReadOrder:false,incidents:[],history:[],slaClock:null,slaPolicies:[]},available=vi.fn(async()=>permitted),nil=()=>null,route=await supportRoute("packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/support/[id]/screen.tsx",{React,requireWorkContext:async()=>actor,publicScope:(value:WorkContext)=>value,workspaceBase:()=>portalBase,...nativeSupportPagePorts(data),supportTicketPortalAvailable:available,pageError:(error:unknown)=>{throw error;},canonicalUiHref:(href:string)=>href,Link:'a',WorkspaceLink:'a',PageContent:'main',Card:'section',InlineNotice:'p',TimeText:nil,SupportSlaConsole:nil,SupportEvidenceConsole:nil,SupportConsole:nil}),Page=route.default as (p:{params:Promise<{holdingSlug:string;id:string}>})=>Promise<React.ReactNode>,html=renderToStaticMarkup(await Page({params:Promise.resolve({holdingSlug:'acme',id:'ticket'})}));expect(html).toContain('Original internal case');expect(html).toContain('Private staff issue');expect(html.includes('Prepare verified customer ticket access')).toBe(permitted);if(permitted)expect(html).toContain('/customers-sales/customers/service/portal?ticketId=ticket');expect(available).toHaveBeenCalledExactlyOnceWith(actor,'ticket');});
it('the original Field Service-managed case does not ask the optional Helpdesk Portal owner or expose a wrong-source action',async()=>{const available=vi.fn(),nil=()=>null,route=await supportRoute("packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/support/[id]/screen.tsx",{React,requireWorkContext:async()=>actor,publicScope:(value:WorkContext)=>value,workspaceBase:()=>portalBase,...nativeSupportPagePorts({row:{id:'ticket',title:'Service-owned case',issue:'Original private issue',status:'open',version:4,source_kind:'field_service'},canManage:true,managedInFieldService:true,canReadOrder:false,incidents:[],history:[],slaClock:null,slaPolicies:[]}),supportTicketPortalAvailable:available,pageError:(error:unknown)=>{throw error;},canonicalUiHref:(href:string)=>href,Link:'a',WorkspaceLink:'a',PageContent:'main',Card:'section',InlineNotice:'p',TimeText:nil,SupportSlaConsole:nil,SupportEvidenceConsole:nil,SupportConsole:nil}),Page=route.default as (p:{params:Promise<{holdingSlug:string;id:string}>})=>Promise<React.ReactNode>,html=renderToStaticMarkup(await Page({params:Promise.resolve({holdingSlug:'acme',id:'ticket'})}));expect(html).toContain('Service-owned case');expect(html).not.toContain('Prepare verified customer ticket access');expect(available).not.toHaveBeenCalled();});


// Run the original Support presenter with a real-shaped Service intake row:
// migration1048 captures service_order_id, while source_record_json/order_id
// remain null. Named visibility is an explicit original owner reader port.
async function actualServiceTicketHandoff(source:Awaited<ReturnType<typeof import("@erpx/server-records/public/source-record-policy")['visibleSourceRecord']>>,options:{orderId?:string|null;managed?:boolean;fail?:Error}={}){const nativeId='service-order-actual-key',read=vi.fn(async()=>{if(options.fail)throw options.fail;return source;}),nil=()=>null,data={row:{id:'ticket-intake',title:'Original Service intake',issue:'Original visit issue',status:'open',version:3,source_kind:options.managed===false?'general':'field_service',service_order_id:options.orderId===undefined?nativeId:options.orderId,service_order_version:2,source_record_json:null,order_id:null,owner_eligible:1},managedInFieldService:options.managed!==false,canManage:false,canReadOrder:false,incidents:[],history:[],slaClock:null,slaPolicies:[]},route=await supportRoute("packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/support/[id]/screen.tsx",{React,requireWorkContext:async()=>actor,publicScope:(value:WorkContext)=>value,workspaceBase:()=>portalBase,...nativeSupportPagePorts(data),visibleSourceRecord:read,supportTicketPortalAvailable:vi.fn(),pageError:(error:unknown)=>{throw error;},canonicalUiHref:(href:string)=>href,Link:'a',WorkspaceLink:'a',PageContent:'main',Card:'section',InlineNotice:'p',TimeText:nil,SupportSlaConsole:nil,SupportEvidenceConsole:nil,SupportConsole:nil}),Page=route.default as (p:{params:Promise<{holdingSlug:string;id:string}>})=>Promise<React.ReactNode>;return{nativeId,read,render:async()=>renderToStaticMarkup(await Page({params:Promise.resolve({holdingSlug:'acme',id:'ticket-intake'})}))};}
const readableServiceVisit={kind:'field_service_order' as const,id:'service-order-actual-key',version:7,href:'/field-service/service-order-actual-key',label:'Field service order · Actual original visit',title:'Actual original visit',companyId:'company',branchId:'branch',departmentId:null,status:'scheduled'};
it('actual Service-owned Support ticket links its exact currently authorized visit even though its legacy source fields are null',async()=>{const c=await actualServiceTicketHandoff(readableServiceVisit),html=await c.render();expect(html).toContain('Open Field Service visit');expect(html).toContain('href="/holdings/acme/field-service/service-order-actual-key"');expect(c.read).toHaveBeenCalledExactlyOnceWith(actor,{kind:'field_service_order',id:c.nativeId});expect(html).not.toContain('/commerce/orders/');expect(html).toContain('Original Service intake');});
it('an unavailable Service destination leaves the original case and owner explanation readable without a guessed visit link',async()=>{const c=await actualServiceTicketHandoff(null),html=await c.render();expect(c.read).toHaveBeenCalledExactlyOnceWith(actor,{kind:'field_service_order',id:c.nativeId});expect(html).toContain('Original Service intake');expect(html).toContain('managed in Field Service');expect(html).not.toContain('Open Field Service visit');});
it.each([{orderId:null},{managed:false}])('the original missing/other-source case never asks for a guessed Service destination %#',async options=>{const c=await actualServiceTicketHandoff(readableServiceVisit,options),html=await c.render();expect(c.read).not.toHaveBeenCalled();expect(html).toContain('Original Service intake');expect(html).not.toContain('Open Field Service visit');});
it('unexpected owner destination failures propagate instead of silently claiming that the visit is unavailable',async()=>{const failure=new Error('Original owner database unavailable'),c=await actualServiceTicketHandoff(null,{fail:failure});await expect(c.render()).rejects.toBe(failure);expect(c.read).toHaveBeenCalledExactlyOnceWith(actor,{kind:'field_service_order',id:c.nativeId});});

// The five Support command routes are native-only. Missing configuration is a
// closed 503 with no retained TypeScript writer call; GET reads keep their
// current owner readers because native reads are a separate migration.
describe("Support command routes require their native configuration", () => {
    const commandHeaders = {'Origin': 'https://helpdesk.test', 'Idempotency-Key': 'command-key'};
    const context = {params: Promise.resolve({holdingSlug: 'acme'})};
    const post = (route: Record<string, unknown>, body: unknown = {action: 'start', id: 'ticket', expectedVersion: 1, reason: 'Current review'}, headers: Record<string, string> = commandHeaders) => (route.POST as (r: Request, c: typeof context) => Promise<Response>)(new Request('https://helpdesk.test/api/owner', {method: 'POST', headers, body: JSON.stringify(body)}), context);
    const get = (route: Record<string, unknown>, url = 'https://helpdesk.test/api/owner') => (route.GET as (r: Request, c: typeof context) => Promise<Response>)(new Request(url), context);
    const runtime = async () => {
        const {ApiError, errorResponse} = await import('@erpx/server-runtime/public/http');
        return {ApiError, errorResponse, canonicalDestinationLinks: <T>(value: T) => value, requestPublicOrigin: (request: Request) => new URL(request.url).origin, requireWorkContext: vi.fn(async () => actor)};
    };
    const writer = (result: unknown = {id: 'native', version: 2}) => {
        const write = vi.fn(async () => result), factory = vi.fn(() => write);
        return {factory, write};
    };
    afterEach(() => vi.unstubAllEnvs());

    it("ticket POST closes without ERPX_NATIVE_SUPPORT_TICKET_CONFIG and never calls a retained writer", async () => {
        const base = await runtime(), legacy = vi.fn(), native = writer();
        const route = await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/route.ts", {...base, supportWorkspace: legacy, supportRecipients: legacy, supportDetail: legacy, createNativeSupportTicketWriter: native.factory, createNativeSupportEvidenceWriter: vi.fn()});
        const response = await post(route, {kind: 'general', title: 'Printer offline', issue: 'The front desk printer is offline.'});
        expect(response.status).toBe(503);
        expect(await response.json()).toMatchObject({error: {code: 'NATIVE_SUPPORT_UNAVAILABLE'}});
        expect(native.factory).not.toHaveBeenCalled();
        expect(legacy).not.toHaveBeenCalled();
        expect(base.requireWorkContext).toHaveBeenCalledWith('acme');
    });

    it("ticket and evidence commands select their exact native writer without a TypeScript fallback", async () => {
        vi.stubEnv('ERPX_NATIVE_SUPPORT_TICKET_CONFIG', 'ticket-config');
        vi.stubEnv('ERPX_NATIVE_SUPPORT_EVIDENCE_CONFIG', undefined);
        const base = await runtime(), legacy = vi.fn(), ticket = writer({id: 'ticket', version: 2}), evidence = writer({id: 'ticket', version: 3});
        const route = await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/route.ts", {...base, supportWorkspace: legacy, supportRecipients: legacy, supportDetail: legacy, createNativeSupportTicketWriter: ticket.factory, createNativeSupportEvidenceWriter: evidence.factory});
        const refused = await post(route, {action: 'bind-evidence', id: 'ticket', expectedVersion: 1, attachmentSetIds: ['set'], reason: 'Reviewed private evidence'});
        expect(refused.status).toBe(503);
        expect(await refused.json()).toMatchObject({error: {code: 'NATIVE_SUPPORT_EVIDENCE_UNAVAILABLE'}});
        expect(ticket.factory).not.toHaveBeenCalled();
        expect(evidence.factory).not.toHaveBeenCalled();
        expect(legacy).not.toHaveBeenCalled();
        const body = {kind: 'general', title: 'Printer offline', issue: 'The front desk printer is offline.'}, created = await post(route, body);
        expect(created.status).toBe(200);
        expect(ticket.factory).toHaveBeenCalledWith('ticket-config');
        expect(ticket.write).toHaveBeenCalledWith(expect.anything(), 'acme', JSON.stringify(body), 'command-key');
        vi.stubEnv('ERPX_NATIVE_SUPPORT_EVIDENCE_CONFIG', 'evidence-config');
        const bound = await post(route, {action: 'bind-evidence', id: 'ticket', expectedVersion: 1, attachmentSetIds: ['set'], reason: 'Reviewed private evidence'});
        expect(bound.status).toBe(200);
        expect(evidence.factory).toHaveBeenCalledWith('evidence-config');
        expect(legacy).not.toHaveBeenCalled();
    });

    it("SLA POST closes without ERPX_NATIVE_SUPPORT_SLA_CONFIG", async () => {
        const base = await runtime(), legacy = vi.fn(), native = writer();
        const route = await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/sla/route.ts", {...base, supportSlaWorkspace: legacy, createNativeSupportSlaWriter: native.factory});
        const response = await post(route, {action: 'create-policy', name: 'Working time', policy: {}, reason: 'Proposed target'});
        expect(response.status).toBe(503);
        expect(await response.json()).toMatchObject({error: {code: 'NATIVE_SUPPORT_SLA_UNAVAILABLE'}});
        expect(native.factory).not.toHaveBeenCalled();
        expect(legacy).not.toHaveBeenCalled();
        vi.stubEnv('ERPX_NATIVE_SUPPORT_SLA_CONFIG', 'sla-config');
        const created = await post(route, {action: 'create-policy', name: 'Working time', policy: {}, reason: 'Proposed target'});
        expect(created.status).toBe(200);
        expect(native.factory).toHaveBeenCalledWith('sla-config');
    });

    it("incident POST closes without ERPX_NATIVE_SUPPORT_INCIDENT_CONFIG", async () => {
        const base = await runtime(), legacy = vi.fn(), native = writer();
        const route = await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/incidents/route.ts", {...base, incidentWorkspace: legacy, incidentDetail: legacy, createNativeSupportIncidentWriter: native.factory});
        const response = await post(route, {action: 'create', ticketId: 'ticket', ticketVersion: 1, title: 'Shared fault', impact: 'Several customers', severity: 'high', ownerId: 'agent', dueDate: '2026-09-27'});
        expect(response.status).toBe(503);
        expect(await response.json()).toMatchObject({error: {code: 'NATIVE_SUPPORT_INCIDENT_UNAVAILABLE'}});
        expect(native.factory).not.toHaveBeenCalled();
        expect(legacy).not.toHaveBeenCalled();
        vi.stubEnv('ERPX_NATIVE_SUPPORT_INCIDENT_CONFIG', 'incident-config');
        expect((await post(route, {action: 'create'})).status).toBe(200);
        expect(native.factory).toHaveBeenCalledWith('incident-config');
    });

    it("session POST closes without ERPX_NATIVE_SUPPORT_SESSION_CONFIG", async () => {
        const base = await runtime(), legacy = vi.fn(), native = writer();
        const route = await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/sessions/route.ts", {...base, supportSessionWorkspace: legacy, supportSessionDetail: legacy, createNativeSupportSessionWriter: native.factory});
        const response = await post(route, {ticketId: 'ticket', ticketVersion: 1, requestedFor: 'agent', expiresAt: Date.now() + 60_000, reason: 'Temporary context'});
        expect(response.status).toBe(503);
        expect(await response.json()).toMatchObject({error: {code: 'NATIVE_SUPPORT_SESSION_UNAVAILABLE'}});
        expect(native.factory).not.toHaveBeenCalled();
        expect(legacy).not.toHaveBeenCalled();
        vi.stubEnv('ERPX_NATIVE_SUPPORT_SESSION_CONFIG', 'session-config');
        expect((await post(route, {action: 'approve', id: 'session', expectedVersion: 1, reason: 'Reviewed'})).status).toBe(200);
        expect(native.factory).toHaveBeenCalledWith('session-config');
    });

    it("GET requires the native read configuration and never calls a retained reader", async () => {
        const base = await runtime(), legacy = vi.fn(), native = writer(), workspace = vi.fn(async () => ({items: []})), reader = vi.fn(() => ({workspace, detail: vi.fn(), recipients: vi.fn()}));
        const route = await supportRoute("packages/feature-support/src/routes/api/v1/holdings/[holdingSlug]/support/route.ts", {...base, createNativeSupportTicketWriter: native.factory, createNativeSupportEvidenceWriter: vi.fn(), createNativeSupportReader: reader});
        const refused = await get(route);
        expect(refused.status).toBe(503);
        expect(await refused.json()).toMatchObject({error: {code: 'NATIVE_SUPPORT_READ_UNAVAILABLE'}});
        expect(reader).not.toHaveBeenCalled();
        expect(legacy).not.toHaveBeenCalled();
        vi.stubEnv('ERPX_NATIVE_SUPPORT_READ_CONFIG', 'read-config');
        const response = await get(route);
        expect(response.status).toBe(200);
        expect(reader).toHaveBeenCalledWith('read-config');
        expect(workspace).toHaveBeenCalledWith(expect.anything(), 'acme');
        expect(native.factory).not.toHaveBeenCalled();
    });
});

it('the Support landing presenter reads only the native workspace and preserves filters, source choice and fail-closed configuration', async () => {
    const workspace = {
        items: [
            { id: 'ticket-open', title: 'Pump pressure low', status: 'open', due_date: '2026-11-06', source_kind: 'asset' },
            { id: 'ticket-closed', title: 'Old printer jam', status: 'closed', due_date: null, source_kind: 'general' },
        ],
        sources: [{ id: 'order-1', number: 'SO-1', version: 2, customer_name: 'Acme Customer' }],
        canCreate: true,
        canCreateOrder: true,
        ownReportsOnly: false,
    };
    const read = vi.fn(async () => workspace),
        recipients = vi.fn(async () => [{ id: 'agent', name: 'Support Agent' }]),
        typed = { kind: 'asset' as const, id: 'asset-1', version: 3, label: 'Asset · Boiler', href: '/assets/asset-1' },
        source = vi.fn(async () => typed),
        rendered: Record<string, unknown>[] = [],
        route = await supportRoute("packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/support/screen.tsx", {
            React,
            requireWorkContext: async () => actor,
            publicScope: (value: WorkContext) => value,
            workspaceBase: () => '/holdings/acme',
            pageError: (error: unknown) => { throw error; },
            createNativeSupportReader: () => ({ workspace: read, recipients }),
            readSourceRecord: source,
            SupportWorkspaceView: (props: Record<string, unknown>) => { rendered.push(props); return React.createElement('div', {}, 'native support workspace'); },
            ApiError: class TestApiError extends Error { readonly status: number; readonly code: string; constructor(status: number, code: string, message: string) { super(message); this.status = status; this.code = code; } },
            headers: async () => new Headers(),
            PageContent: (props: { children?: React.ReactNode }) => React.createElement('main', {}, props.children),
        }),
        Page = route.default as (props: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<Record<string, string>> }) => Promise<React.ReactNode>;
    vi.stubEnv('ERPX_NATIVE_SUPPORT_READ_CONFIG', 'native-support-read');
    const html = renderToStaticMarkup(await Page({
        params: Promise.resolve({ holdingSlug: 'acme' }),
        searchParams: Promise.resolve({ new: '1', orderId: 'order-1', sourceKind: 'asset', sourceId: 'asset-1', sourceVersion: '3', status: 'open', q: 'pump' }),
    }));
    expect(html).toContain('native support workspace');
    expect(read).toHaveBeenCalledExactlyOnceWith(expect.anything(), 'acme');
    expect(recipients).toHaveBeenCalledExactlyOnceWith(expect.anything(), 'acme', 'order-1');
    expect(source).toHaveBeenCalledExactlyOnceWith(actor, { kind: 'asset', id: 'asset-1', version: 3 });
    expect(rendered[0]).toMatchObject({
        base: '/holdings/acme',
        data: workspace,
        query: { new: '1', orderId: 'order-1', sourceKind: 'asset', sourceId: 'asset-1', sourceVersion: '3', status: 'open', q: 'pump' },
        recipients: [{ id: 'agent', name: 'Support Agent' }],
        source: { id: 'order-1', version: 2 },
        typedSource: { kind: 'asset', id: 'asset-1', version: 3, label: 'Asset · Boiler' },
    });
    const sourceText = await (await import('node:fs/promises')).readFile(new URL('../../../packages/workplace/src/routes/holdings/[holdingSlug]/_presenters/support/screen.tsx', import.meta.url), 'utf8');
    expect(sourceText).toContain('@erpx/feature-support/lib/native-support-read');
    expect(sourceText).not.toContain('@erpx/server-sales');
    expect(sourceText).not.toContain('supportWorkspace');
    vi.unstubAllEnvs();
    const missing = route.default as typeof Page;
    await expect(missing({ params: Promise.resolve({ holdingSlug: 'acme' }), searchParams: Promise.resolve({}) })).rejects.toMatchObject({ status: 503, code: 'NATIVE_SUPPORT_READ_UNAVAILABLE' });
});

it('the native Support workspace preserves filtered rows, source choice, typed evidence and bounded pagination', async () => {
    const rows = Array.from({ length: 31 }, (_, index) => ({
        id: `ticket-${index + 1}`,
        title: index === 0 ? 'Pump pressure low' : index === 30 ? 'Old printer jam' : `Service case ${index + 1}`,
        status: 'open',
        due_date: index === 30 ? null : '2026-11-06',
        source_kind: 'general',
    }));
    const data = { items: rows, sources: [{ id: 'order-1', number: 'SO-1', version: 2, customer_name: 'Acme Customer' }], canCreate: true, canCreateOrder: true, ownReportsOnly: false },
        components = await supportRoute('packages/feature-support/src/components/support-workspace.tsx', {
            React,
            canonicalUiHref: (href: string) => `canonical:${href}`,
            Link: 'a',
            SupportConsole: (props: { data: { canCreate: boolean }; typedSource?: { label: string } }) => React.createElement('div', { 'data-support-console': String(props.data.canCreate) }, props.typedSource?.label ?? ''),
            ActionLink: (props: { href: string; children?: React.ReactNode }) => React.createElement('a', { href: props.href }, props.children),
            Button: (props: { type?: string; children?: React.ReactNode }) => React.createElement('button', { type: props.type }, props.children),
            Field: (props: { children?: React.ReactNode }) => React.createElement('label', {}, props.children),
            Select: (props: { name: string; defaultValue?: string; children?: React.ReactNode }) => React.createElement('select', { name: props.name, defaultValue: props.defaultValue }, props.children),
            Table: 'table',
            TableFrame: (props: { children?: React.ReactNode }) => React.createElement('div', {}, props.children),
            TextInput: (props: { name: string; defaultValue?: string }) => React.createElement('input', { name: props.name, defaultValue: props.defaultValue }),
            FormDisclosure: (props: { open?: boolean; children?: React.ReactNode }) => React.createElement('details', { open: props.open }, props.children),
            StatusBadge: (props: { status: string }) => React.createElement('span', {}, props.status),
        }),
        View = components.SupportWorkspaceView as (props: Record<string, unknown>) => React.ReactNode;
    expect(renderToStaticMarkup(React.createElement(View, {
        base: '/holdings/acme',
        data,
        query: { status: 'open', q: 'pump', page: '1', new: '1', orderId: 'order-1', sourceKind: 'asset', sourceId: 'asset-1', sourceVersion: '3' },
        recipients: [{ id: 'agent', name: 'Support Agent' }],
        source: { id: 'order-1', version: 2 },
        typedSource: { kind: 'asset', id: 'asset-1', version: 3, label: 'Asset · Boiler' },
    }))).toContain('Pump pressure low');
    const filtered = renderToStaticMarkup(React.createElement(View, {
        base: '/holdings/acme',
        data,
        query: { status: 'open', q: 'pump', page: '1' },
        recipients: [],
    }));
    expect(filtered).toContain('Pump pressure low');
    expect(filtered).not.toContain('Service case 2');
    expect(filtered).not.toContain('Old printer jam');
    expect(filtered).toContain('canonical:/holdings/acme/support?new=1');
    expect(filtered).toContain('1 case');
    const paged = renderToStaticMarkup(React.createElement(View, { base: '/holdings/acme', data, query: { status: 'open', page: '1' }, recipients: [] }));
    expect(paged).toContain('Service case 30');
    expect(paged).not.toContain('Old printer jam');
    expect(paged).toContain('canonical:/holdings/acme/support?page=2');
    const created = renderToStaticMarkup(React.createElement(View, {
        base: '/holdings/acme',
        data,
        query: { new: '1', orderId: 'order-1', sourceKind: 'asset', sourceId: 'asset-1', sourceVersion: '3' },
        recipients: [{ id: 'agent', name: 'Support Agent' }],
        source: { id: 'order-1', version: 2 },
        typedSource: { kind: 'asset', id: 'asset-1', version: 3, label: 'Asset · Boiler' },
    }));
    expect(created).toContain('Choose source');
    expect(created).toContain('data-support-console="false"');
    expect(created).toContain('Asset · Boiler');
    expect(created).toContain('canonical:/holdings/acme/support/sla');
    expect(created).toContain('canonical:/holdings/acme/support/incidents');
});

it('the Support case routes delegate to the native Support presenter instead of the Sales customer hub', async () => {
    const { readFile } = await import('node:fs/promises');
    for (const path of [
        'packages/feature-support/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/cases/page.tsx',
        'packages/feature-support/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/cases/[id]/page.tsx',
        'packages/feature-support/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/cases/[id]/[recordSection]/page.tsx',
    ]) {
        const source = await readFile(new URL(`../../../${path}`, import.meta.url), 'utf8');
        expect(source).toContain('_presenters/support/screen');
        expect(source).not.toContain('_presenters/customers/screen');
        expect(source).not.toContain('customers/service/cases/_screen');
    }
    const Screen = vi.fn(async () => React.createElement('div', {}, 'native workspace'));
    const route = await supportRoute("packages/feature-support/src/routes/holdings/[holdingSlug]/customers-sales/customers/service/cases/page.tsx", {
        Screen,
        notFound: () => { throw new Error('not found'); },
        recordSections: {},
        defaultRecordSection: () => 'overview',
    });
    const html = renderToStaticMarkup(await (route.default as (props: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<Record<string, string>> }) => Promise<React.ReactNode>)({
        params: Promise.resolve({ holdingSlug: 'acme' }),
        searchParams: Promise.resolve({ status: 'open' }),
    }));
    expect(html).toContain('native workspace');
    expect(Screen).toHaveBeenCalledOnce();
    await expect((Screen.mock.calls[0] as unknown as [{ params: Promise<{ holdingSlug: string }>; searchParams: Promise<Record<string, string>> }])[0].searchParams).resolves.toEqual({ status: 'open' });
});
