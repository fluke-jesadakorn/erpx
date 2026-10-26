import { describe, expect, it,vi } from "vitest";
vi.mock("next/navigation",()=>({useRouter:()=>({push:vi.fn(),replace:vi.fn(),refresh:vi.fn()}),usePathname:()=>"/holdings/acme/people-organization/people",useSearchParams:()=>new URLSearchParams()}));
import { peopleContextHref, peopleSettingsHref } from "@erpx/feature-people_payroll/lib/people-route-context";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { RosterOwnerWorkspace as RosterWorkspace } from "@erpx/feature-people_payroll/components/roster-owner-workspace";
import type { RosterData } from "@erpx/feature-people_payroll/lib/people-roster";
import { createRosterEditorDraft, currentRosterSnapshot, rosterCommandAcknowledgement } from "@erpx/feature-people_payroll/lib/roster-editor-draft";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import {captureWorkCommandContext} from '@erpx/feature-work_projection/lib/command-context';
import {setContextSwitching} from '@erpx/feature-work_projection/lib/context-switch';

describe("People route context", () => {
  it("retains repeated filters, roster request, and week through canonical navigation", () => {
    const href = peopleContextHref("/holdings/acme", {
      department: "operations", week: "2026-09-28", view: "mine", request: "request-1",
      filter: ["leave", "shift"], ignored: undefined,
    }, { panel: "schedule" });
    const url = new URL(href, "https://people.invalid");
    expect(url.pathname).toBe("/holdings/acme/people-organization/people/roster");
    expect(url.searchParams.get("panel")).toBeNull();
    expect(url.searchParams.get("request")).toBe("request-1");
    expect(url.searchParams.get("week")).toBe("2026-09-28");
    expect(url.searchParams.getAll("filter")).toEqual(["leave", "shift"]);
    expect(url.searchParams.has("ignored")).toBe(false);
  });

  it("keeps pending moves without inventing a destination and scopes draft review", () => {
    const pending = new URL(peopleContextHref("/holdings/acme", { kind: "person", id: "person-1", draft: "private-1" }, { panel: "move" }), "https://people.invalid");
    expect(pending.searchParams.get("kind")).toBe("person");
    expect(pending.searchParams.has("targetId")).toBe(false);
    const review = new URL(peopleContextHref("/holdings/acme/branches/main", { panel: "move", draft: "other", q: "สมชาย" }, { panel: "review", draft: "private-1" }), "https://people.invalid");
    expect(review.pathname).toBe("/holdings/acme/branches/main/people-organization/people");
    expect(review.searchParams.getAll("draft")).toEqual(["private-1"]);
    expect(review.searchParams.get("q")).toBe("สมชาย");
  });
  it("moves settings to a full page while preserving repeated context and an editor", () => {
    const url = new URL(peopleSettingsHref("/holdings/acme", {panel:"settings",section:"leave",editor:"policy",unit:"department:bangkok:people",filter:["one","two"]}), "https://people.invalid");
    expect(url.pathname).toBe("/holdings/acme/people-organization/people/hr-settings/leave");
    expect(url.searchParams.has("panel")).toBe(false);
    expect(url.searchParams.get("editor")).toBe("policy");
    expect(url.searchParams.get("unit")).toBe("department:bangkok:people");
    expect(url.searchParams.getAll("filter")).toEqual(["one","two"]);
  });

});

it('Roster keeps its original uncertain command and newer dirty draft when authority changes during real response decoding',async()=>{
 const node={isConnected:true,dataset:{workUser:'original-user',workRevision:'7',workCompany:'original-company',workAssignment:'original-assignment'}},scope=()=>Object.values(node.dataset).join(':');
 vi.stubGlobal('window',Object.assign(new EventTarget(),{location:{origin:'https://people-dev-erpx.jesadakorn.com'}}));
 vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?node:null});
 let signalBody!:()=>void,release!:ReadableStreamDefaultController<Uint8Array>;
 const bodyStarted=new Promise<void>(resolve=>{signalBody=resolve;}),body=new ReadableStream<Uint8Array>({start(controller){release=controller;}});
 class LazyResponse extends Response{override json(){signalBody();return super.json();}}
 let calls=0;
 const send=vi.fn<(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{
  if(++calls===1)throw Error('Lost response');
  return calls===2?new LazyResponse(body,{headers:{'content-type':'application/json'}}):Response.json({data:{id:'original-template',version:2}});
 });vi.stubGlobal('fetch',send);
 try{
  const draft=createRosterEditorDraft(()=> 'captured-key');draft.open();draft.changed();
  const input=JSON.stringify({action:'template.save',id:'original-template',expectedVersion:1,name:'Submitted draft'}),endpoint='/api/v1/holdings/acme/people/roster',attempt=draft.begin(endpoint,input,scope());
  if(!attempt)throw Error('The original Roster attempt is required.');
  const transport=()=>workFetch(endpoint,{method:'POST',headers:{'Content-Type':'application/json','Idempotency-Key':attempt.key},body:attempt.body},undefined,'caller');
  await expect(transport()).rejects.toThrow('Lost response');draft.failed(attempt,true);draft.changed();
  const retry=draft.begin(endpoint,input,scope());expect(retry).toBe(attempt);
  const decoded=transport().then(response=>response.json());await bodyStarted;node.dataset.workRevision='8';
  release.enqueue(new TextEncoder().encode(JSON.stringify({data:{id:'original-template',version:2}})));release.close();
  const json=await decoded;expect(rosterCommandAcknowledgement(json.data)).toEqual({id:'original-template',version:2});
  expect(()=>draft.acknowledged(attempt,scope())).toThrow('account, Company or role changed');
  draft.failed(attempt,true);expect(draft.isDirty()).toBe(true);expect(()=>draft.begin(endpoint,input,scope())).toThrow('Retry the submitted change');
  node.dataset.workRevision='7';expect(draft.begin(endpoint,input,scope())).toBe(attempt);
  expect(rosterCommandAcknowledgement((await (await transport()).json()).data)).toEqual({id:'original-template',version:2});
  expect(draft.acknowledged(attempt,scope())).toBe(false);expect(draft.isDirty()).toBe(true);expect(draft.pending()).toBe(false);
  expect(send.mock.calls.map(call=>call[1]?.body)).toEqual([input,input,input]);expect(send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['captured-key','captured-key','captured-key']);
 }finally{vi.unstubAllGlobals();}
});

it.each(['stale boundary','replaced boundary','switch generation'] as const)('the actual Roster handler retains its original uncertain command and newer edit after a late %s',async changed=>{
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL("../../../packages/feature-people_payroll/src/components/roster-owner-workspace.tsx",import.meta.url),'utf8'),file=ts.createSourceFile('actual-roster-owner.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),functions:import('typescript').FunctionDeclaration[]=[];
 const visit=(node:import('typescript').Node)=>{if(ts.isFunctionDeclaration(node)&&['send','apply'].includes(node.name?.text??''))functions.push(node);ts.forEachChild(node,visit);};visit(file);if(functions.length!==2)throw Error('Expected the actual Roster owner send and apply handlers.');
 const emitted=ts.transpileModule(functions.map(fn=>fn.getText(file)).join('\n'),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const {announceWorkChange}=await import('@erpx/feature-work_projection/lib/work-refresh');
 const node={isConnected:true,dataset:{workUser:'operator',workRevision:'7',workCompany:'company',workAssignment:'assignment',workStale:'false'}};let currentNode=node,edit='submitted-edit',calls=0;
 const form={isConnected:true,getAttribute:()=>edit},browser=Object.assign(new EventTarget(),{location:{origin:'https://people-dev-erpx.jesadakorn.com'}}),events:string[]=[];
 browser.addEventListener('erpx:work-changed',()=>events.push('saved'));vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?currentNode:null,activeElement:{closest:()=>form},documentElement:{dataset:{}}});
 let release!:ReadableStreamDefaultController<Uint8Array>,signalBody!:()=>void;const bodyStarted=new Promise<void>(resolve=>{signalBody=resolve;}),body=new ReadableStream<Uint8Array>({start(controller){release=controller;}});
 class LazyResponse extends Response{override json(){signalBody();return super.json();}}
 const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{if(++calls===1)throw Error('Original response lost');return calls===2?new LazyResponse(body,{headers:{'content-type':'application/json'}}):Response.json({data:{id:'template',version:2}});});vi.stubGlobal('fetch',network);
 const state:Record<string,unknown[]>={},setter=(key:string)=>(value:unknown)=>{(state[key]??=[]).push(value);},load=vi.fn(async()=>{}),data=roster(),input={action:'template.save',id:'template',expectedVersion:1,departmentId:'department',name:'Submitted template',startTime:'06:00',endTime:'14:00',capacity:2,active:true};
 const commands=createRosterOwnerCommands(workFetch,()=> 'original-key'),draft={current:commands};commands.bind(()=>({base:'/holdings/acme',data,initial:data,view:'templates'}));commands.open();
 const names=['commands','completedEditor','busy','refreshRequired','submittedEditor','editorRoot','announceWorkChange','editor','data','load','notifyDraft','setDirty','setBusy','setError','setMessage','setPending','setEditor','setEditorOpen','setCompletedEditor','setRefreshRequired','input'],run=new Function(...names,`${emitted}\nreturn commands.hasPending()?apply(commands.retry):send(input);`) as (...ports:unknown[])=>Promise<void>;
 // The retained-request assertion observes the REAL current owner pending state
 // and actual sent body, replacing only the obsolete component's setRetry port.
 const submit=async()=>{await run(commands,false,false,null,{current:{kind:'template',template:data.templates[0]}},{current:{querySelector:()=>form}},announceWorkChange,{kind:'template',template:data.templates[0]},data,load,()=>{},...['dirty','busy','error','message','pending','editor','editorOpen','completedEditor','refreshRequired'].map(setter),input);(state.retry??=[]).push(commands.hasPending()?{body:network.mock.calls.at(-1)?.[1]?.body}:null);};
 try{
  await submit();expect(state.error.at(-1)).toBe('Original response lost');draft.current.changed();edit='newer-edit';
  const retry=submit();await bodyStarted;if(changed==='stale boundary')node.dataset.workStale='true';else if(changed==='replaced boundary'){node.isConnected=false;currentNode={isConnected:true,dataset:{...node.dataset}};}else setContextSwitching(true);
  release.enqueue(new TextEncoder().encode(JSON.stringify({data:{id:'template',version:2}})));release.close();await retry;
  expect(events).toEqual([]);expect(load).not.toHaveBeenCalled();expect(state.error.at(-1)).toMatch(/context|role/i);expect(state.retry.at(-1)).toMatchObject({body:JSON.stringify(input)});expect(draft.current.isDirty()).toBe(true);
  node.dataset.workStale='false';node.isConnected=true;currentNode=node;setContextSwitching(false);await submit();expect(events).toEqual(['saved']);expect(load).toHaveBeenCalledOnce();expect(draft.current.isDirty()).toBe(true);expect(state.retry.at(-1)).toBeNull();expect(state.message.at(-1)).toContain('newer draft is retained');
  expect(network.mock.calls.map(call=>call[1]?.body)).toEqual([JSON.stringify(input),JSON.stringify(input),JSON.stringify(input)]);expect(network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['original-key','original-key','original-key']);
 }finally{setContextSwitching(false);vi.unstubAllGlobals();}
});

function roster(purchase?: RosterData["purchase"]): RosterData {
  return {purchase, asOf: 1, departments: [{id: "department", name: "Operations", branch_id: null, branch_name: null}], departmentId: "department", weekStart: "2026-10-19",
    week: {id: "week", company_id: "company", branch_id: null, department_id: "department", week_start: "2026-10-19", timezone: "Asia/Bangkok", state: "published", version: 1},
    weeks: [{id: "week", company_id: "company", branch_id: null, department_id: "department", week_start: "2026-10-19", timezone: "Asia/Bangkok", state: "published", version: 1}],
    shifts: [{id: "shift", week_id: "week", name: "Morning", local_date: "2026-10-19", start_time: "06:00", end_time: "14:00", starts_at: 100, ends_at: 200, capacity: 2, state: "active", version: 1, template_id: null}],
    assignments: [], requests: [], templates: [{id: "template", department_id: "department", name: "Morning template", start_time: "06:00", end_time: "14:00", capacity: 2, active: true, version: 1}], people: [], coverageGaps: [], history: [], conflicts: [], selfPersonId: "person", selfRosterRequired: true, canManage: true, canApprove: true};
}
const renderRoster = (initial: RosterData, initialView = "team") => renderToStaticMarkup(createElement(RosterWorkspace, {initial, base: "/holdings/acme/people-organization/people", embedded: true, initialView}));
describe("Roster purchase presentation", () => {
  it("renders original active planning and self-service controls from the initial server snapshot", () => {
    for (const data of [roster(), roster({newWork: true, settleExisting: true, history: true, revision: "active"})]) {
      const html = renderRoster(data);
      expect(html).toContain("Add shift"); expect(html).toContain("Edit shift"); expect(html).toContain("Request slot");
      expect(renderRoster(data, "templates")).toContain("New template");
    }
  });
  it("keeps retained shifts and cancellation visible during draining without planning or new requests", () => {
    const data = roster({newWork: false, settleExisting: true, history: true, revision: "draining"}), html = renderRoster(data);
    expect(html).toContain("Morning"); expect(html).toContain("Cancel shift");
    for (const control of ["Add shift", "Edit shift", "Request slot", "Request swap", "Copy week", "Create week"]) expect(html).not.toContain(control);
    const templates = renderRoster(data, "templates"); expect(templates).toContain("Morning template"); expect(templates).not.toContain("New template"); expect(templates).not.toContain("Edit template");
    expect(html).not.toContain("package_grants"); expect(html).not.toContain("sourceEventId");
  });
  it("renders retained read-only history after settlement access ends", () => {
    const html = renderRoster(roster({newWork: false, settleExisting: false, history: true, revision: "expired"}));
    expect(html).toContain("Morning"); expect(html).not.toContain("Cancel shift"); expect(html).not.toContain("Request slot");
  });
});

describe("Roster editor acknowledgements and recovery", () => {
  const context = () => {let key = 0; return createRosterEditorDraft(() => `command-${++key}`);};
  it("retains dirty input until the exact submitted edit is acknowledged", () => {
    const draft = context(); draft.open(); draft.changed();
    expect(draft.isDirty()).toBe(true);
    const submitted = draft.begin("/roster", '{"name":"Morning"}', "role-A")!;
    expect(draft.pending()).toBe(true); expect(draft.begin("/roster", submitted.body, "role-A")).toBeNull();
    expect(draft.acknowledged(submitted)).toBe(true); expect(draft.isDirty()).toBe(false); expect(draft.pending()).toBe(false);
  });
  it("keeps newer edits dirty when an earlier save or retry completes", () => {
    const draft = context(); draft.open(); draft.changed();
    const submitted = draft.begin("/roster", '{"name":"Submitted"}', "role-A")!;
    draft.failed(submitted, true); draft.changed();
    const retry = draft.begin("/roster", submitted.body, "role-A")!;
    expect(retry).toBe(submitted); expect(retry.key).toBe(submitted.key);
    expect(draft.acknowledged(retry)).toBe(false); expect(draft.isDirty()).toBe(true);
    const newer = draft.begin("/roster", '{"name":"Newer retained draft"}', "role-A")!;
    expect(newer.key).not.toBe(submitted.key); expect(draft.acknowledged(newer)).toBe(true);
  });
  it("retains uncertain payload identity and prevents a different command or role from becoming its retry", () => {
    const draft = context(); draft.open();
    const submitted = draft.begin("/roster", '{"name":"Created"}', "role-A")!;
    draft.failed(submitted, true);
    for (const [url, body, scope] of [["/roster", '{"name":"Different"}', "role-A"], ["/roster", submitted.body, "role-B"], ["/another-company/roster", submitted.body, "role-A"]])
      expect(() => draft.begin(url, body, scope)).toThrow("Retry the submitted change");
    expect(draft.isDirty()).toBe(true);
    const retry = draft.begin("/roster", submitted.body, "role-A")!;
    expect(retry.key).toBe(submitted.key); expect(retry.body).toBe(submitted.body);
  });
  it("allows correction after a confirmed rejection and ignores old editor acknowledgements after explicit discard", () => {
    const draft = context(); draft.open(); draft.changed();
    const failed = draft.begin("/roster", "invalid", "role-A")!;
    draft.failed(failed, false);
    const correction = draft.begin("/roster", "corrected", "role-A")!;
    expect(correction.key).not.toBe(failed.key);
    draft.open(); draft.changed();
    expect(draft.acknowledged(correction)).toBe(false); expect(draft.isDirty()).toBe(true);
  });
  it("requires a complete owner acknowledgement before clearing any draft", () => {
    for (const value of [undefined, null, {}, {id: "template"}, {id: "template", version: 0}, {id: "", version: 1}, {id: "template", version: 1.5}])
      expect(() => rosterCommandAcknowledgement(value)).toThrow("acknowledgement is incomplete");
    expect(rosterCommandAcknowledgement({id: "template", version: 2, state: "published"})).toEqual({id: "template", version: 2});
  });
  it("keeps a newer Roster snapshot when an earlier read finishes after it", () => {
    const current = {asOf: 200, name: "Current"}, earlier = {asOf: 100, name: "Earlier"};
    expect(currentRosterSnapshot(current, earlier)).toBe(current);
    expect(currentRosterSnapshot(current, {...earlier, asOf: 300}).name).toBe("Earlier");
  });
});

import {LifecyclePanel} from '@erpx/feature-people_payroll/components/lifecycle-workspace';
import type {PeopleLifecycleData} from '@erpx/feature-people_payroll/lib/people-lifecycle';
import type {PeoplePurchaseControls} from '@erpx/feature-people_payroll/lib/purchase-controls';
function lifecycleControls(newWork:boolean,settleExisting:boolean):PeoplePurchaseControls{const access={newWork,settleExisting,history:true,revision:'public-observation'};return {directory:access,lifecycle:access,identity:access};}
function renderLifecycle(purchases?:PeoplePurchaseControls,status='active'){
 const data:PeopleLifecycleData={purchases,roster:[],managers:[],nodes:[],packages:[],positions:[],cases:[],tasks:[],settings:{version:0,itAssigneeId:null,onboardingTasks:[],offboardingTasks:[]},canManage:true,canPrepare:false,canReview:true};return renderToStaticMarkup(createElement(LifecyclePanel,{base:'/holdings/acme',data,person:{id:'person',engagementId:'engagement',positionId:null,managerPersonId:null,name:'Original employee',status}}));
}
describe('People lifecycle purchase presentation',()=>{
 it('preserves active and default-disabled incoming and departure controls',()=>{for(const controls of [undefined,lifecycleControls(true,true)]){const html=renderLifecycle(controls);expect(html).toContain('Start a change');expect(html).toContain('Retire employee');expect(renderLifecycle(controls,'ended')).toContain('Create a new engagement for rehire');}});
 it('keeps retained departure work during draining and hides incoming and rehire controls',()=>{const controls=lifecycleControls(false,true),html=renderLifecycle(controls);expect(html).toContain('Retire employee');expect(html).not.toContain('Start a change');expect(renderLifecycle(controls,'ended')).not.toContain('Create a new engagement for rehire');expect(html).not.toContain('company_package_grants');expect(html).not.toContain('authorizingAssignmentId');});
 it('retains employee history after settlement ends without a mutation control',()=>{const html=renderLifecycle(lifecycleControls(false,false));expect(html).toContain('Employment &amp; position');expect(html).not.toContain('Start a change');expect(html).not.toContain('Retire employee');});
});

// Standard Directory keeps normal native owner commands, not a replacement API.
import {readFileSync as directoryRead} from "node:fs";
import {createRequire as directoryRequire} from "node:module";
import {runInNewContext as directoryEvaluate} from "node:vm";
import * as directoryReact from "react";
import * as directoryContext from "@erpx/feature-work_projection/lib/context-switch";
import * as directoryRetirement from "@erpx/feature-people_payroll/lib/retirement";
import * as directoryPurchase from "@erpx/feature-people_payroll/lib/purchase-controls";
import {lifecycleInput as directoryLifecycleInput, hasLifecycleCaseDetail as directoryHasDetail} from "@erpx/feature-people_payroll/lib/people-lifecycle";
import {createDirectoryOwnerCommand, type DirectoryCommandCurrent} from "@erpx/feature-people_payroll/lib/directory-owner-command";
import type {workFetch as directoryFetch} from "@erpx/feature-work_projection/lib/work-fetch";
const directoryPreview = {token: "native-preview", ready: true, blockers: [], definition: null, removed: [], added: [], accounts: [], affectedPositions: [], affectedPeople: ["Employee"], affectedComputers: []};
function directoryDeferred<Value>() {let resolve!: (value: Value) => void; const promise = new Promise<Value>(done => {resolve = done;}); return {promise, resolve};}
const directoryTick = async () => {for (let index = 0; index < 15; index++) await Promise.resolve();};
function directoryBrowser() {
  const node = {dataset: {workUser: "hr", workCompany: "company", workAssignment: "assignment", workRevision: "v1", workStale: "false"}, isConnected: true}, events: Event[] = [], target = new EventTarget(), dispatch = target.dispatchEvent.bind(target);
  vi.stubGlobal("window", Object.assign(target, {location: {origin: "https://people.example.test", href: "https://people.example.test/holdings/acme/people-organization/people/directory/current?filter=one&filter=two&archived.q=old&archived.cursor=cursor&archived.person=person"}, dispatchEvent: (event: Event) => {events.push(event); return dispatch(event);}}));
  vi.stubGlobal("document", {querySelector: (selector: string) => selector === "[data-work-revision]" ? node : null, activeElement: null});
  return {node, events, cleanup: () => vi.unstubAllGlobals()};
}
function directoryReply(data: unknown, duringJson?: () => void | Promise<void>, status = 200) {
  const response = Response.json(status === 200 ? {data} : {error: data}, {status}), original = response.json.bind(response);
  response.json = async () => {await duringJson?.(); return original();}; return response;
}
function directoryCommandFixture(family: "directory" | "lifecycle" = "lifecycle") {
  const browser = directoryBrowser(), current: DirectoryCommandCurrent = {base: "/holdings/acme", ownerKey: "person", source: "ssr1", canManage: true, canReview: true, personId: "person", engagementId: "engagement", recordId: "person", draftVersion: 1, hasDraft: false, purchases: lifecycleControls(true, true), cases: [{id: "case", kind: "transfer"}]};
  const send = vi.fn<typeof directoryFetch>(async (url, init) => {
    const key = new Headers(init?.headers).get("Idempotency-Key"), input = JSON.parse(String(init?.body));
    if (String(url).endsWith("/preview")) return directoryReply(directoryPreview);
    if (String(url).endsWith("/cancel")) return directoryReply({id: "case", status: "cancelled"});
    if (String(url).endsWith("/rehire")) return directoryReply({id: `eng-rehire-${key}`, personId: input.personId, status: "onboarding"});
    return directoryReply({id: family === "directory" ? "person" : input.id ?? `lifecycle-${key}`, version: Number(input.expectedVersion ?? 0) + 1, status: String(url).endsWith("/draft") ? "draft" : "scheduled"});
  });
  const commands = createDirectoryOwnerCommand(family, send); commands.bind(() => current);
  const input = directoryLifecycleInput.parse({personId: "person", engagementId: "engagement", kind: "transfer", positionId: "position", effectiveAt: 1792296000000, expectedVersion: 0, reason: "Normal reviewed placement"});
  return {...browser, current, send, commands, input};
}
it("retains the first lifecycle request when Work changes during ORIGINAL JSON and recovers exact body/key under current history", async () => {
  const f = directoryCommandFixture(); try {
    f.send.mockImplementationOnce(async (_url, init) => directoryReply({id: `lifecycle-${new Headers(init?.headers).get("Idempotency-Key")}`, version: 1, status: "scheduled"}, () => {f.node.dataset.workRevision = "v2";}));
    await expect(f.commands.send("placements/submit", f.input)).rejects.toMatchObject({code: "WORK_CONTEXT_CHANGED"});
    expect(f.commands.hasPending()).toBe(true); expect(f.events).toHaveLength(0);
    f.current.source = "current-retained-case"; f.current.purchases = lifecycleControls(false, false);
    const receipt = await f.commands.retry(); expect(receipt.acknowledge()).toBe(true);
    expect(f.send.mock.calls[1][1]?.body).toBe(f.send.mock.calls[0][1]?.body);
    expect(new Headers(f.send.mock.calls[1][1]?.headers).get("Idempotency-Key")).toBe(new Headers(f.send.mock.calls[0][1]?.headers).get("Idempotency-Key"));
    expect(f.send.mock.calls.every(call => call[3] === "caller")).toBe(true);
  } finally {f.cleanup();}
});
it("keeps uncertain employment body/key through later edits and 403 without admitting a replacement request", async () => {
  const f = directoryCommandFixture(); try {
    f.send.mockRejectedValueOnce(Error("Lost native response")); await expect(f.commands.send("placements/submit", f.input)).rejects.toThrow("Lost native response");
    await expect(f.commands.send("placements/submit", {...f.input, reason: "Newer draft"})).rejects.toThrow("Recover the original");
    f.send.mockResolvedValueOnce(directoryReply({code: "FORBIDDEN", message: "Current HR authority unavailable"}, undefined, 403));
    await expect(f.commands.retry()).rejects.toThrow("Current HR authority unavailable"); expect(f.commands.hasPending()).toBe(true);
    expect((await f.commands.retry()).acknowledge()).toBe(true);
    expect(f.send.mock.calls.slice(1).every(call => call[1]?.body === f.send.mock.calls[0][1]?.body)).toBe(true);
  } finally {f.cleanup();}
});
it("guards the last acknowledgement after helper return and never clears pending on a late source change", async () => {
  const f = directoryCommandFixture(); try {
    const receipt = await f.commands.send("placements/submit", f.input); expect(f.events).toHaveLength(0);
    f.current.source = "new-owner-snapshot"; expect(() => receipt.acknowledge()).toThrow("owner"); expect(f.commands.hasPending()).toBe(true);
    expect((await f.commands.retry()).acknowledge()).toBe(true); expect(f.events.filter(event => event.type === "erpx:record-command-saved")).toHaveLength(1);
  } finally {f.cleanup();}
});
it("acknowledges only the native case identity/version/status and keeps newer submitted-form edits", async () => {
  const f = directoryCommandFixture(); try {
    f.send.mockResolvedValueOnce(directoryReply({id: "another-case", version: 1, status: "scheduled"}));
    await expect(f.commands.send("placements/submit", f.input)).rejects.toThrow("acknowledgement is incomplete"); expect(f.commands.hasPending()).toBe(true);
    (await f.commands.retry()).acknowledge();
    const state = {version: "original"}, form = {isConnected: true, getAttribute: () => state.version} as unknown as HTMLFormElement;
    const receipt = await f.commands.send("placements/draft", f.input, {form, formEditVersion: "original"});
    state.version = "newer"; expect(receipt.unchanged()).toBe(false); expect(receipt.acknowledge()).toBe(true);
    const saved = f.events.filter(event => event.type === "erpx:record-command-saved").at(-1) as CustomEvent;
    expect(saved.detail).toEqual({form, formEditVersion: "original"});
  } finally {f.cleanup();}
});
it("preserves newer lifecycle review edits when a step-two save completes after returning to the form", async () => {
  const f = directoryCommandFixture(); try {
    const receipt = await f.commands.send("placements/submit", f.input); f.current.draftVersion = 2;
    expect(receipt.unchanged()).toBe(false); receipt.acknowledge(); expect(f.commands.hasPending()).toBe(false);
  } finally {f.cleanup();}
});
it("retains rehire replay after the same Person's current engagement advances without changing its original body", async () => {
  const f = directoryCommandFixture(); try {
    f.send.mockRejectedValueOnce(Error("Rehire response lost")); await expect(f.commands.send("rehire", {personId: "person", engagementId: "engagement", positionId: "position", startDate: "2026-10-20", reason: "Normal reviewed rehire"})).rejects.toThrow("Rehire response lost");
    f.current.engagementId = "new-current-engagement"; f.current.source = "new-history";
    const receipt = await f.commands.retry(); expect(receipt.acknowledge()).toBe(true);
    expect(f.send.mock.calls[1][1]?.body).toBe(f.send.mock.calls[0][1]?.body);
  } finally {f.cleanup();}
});
it("keeps native draining departure/cancel duties while refusing incoming, foreign Person and unsupported owner paths", async () => {
  const f = directoryCommandFixture(); try {
    f.current.purchases = lifecycleControls(false, true);
    await expect(f.commands.send("placements/submit", f.input)).rejects.toThrow("purchase or role");
    (await f.commands.send("placements/submit", {...f.input, kind: "offboarding"})).acknowledge();
    (await f.commands.send("cases/case/cancel", {expectedVersion: 2})).acknowledge();
    await expect(f.commands.send("placements/draft", {...f.input, personId: "foreign"})).rejects.toThrow("purchase or role");
    await expect(f.commands.send("settings", {})).rejects.toThrow("Unsupported");
    expect(f.send).toHaveBeenCalledTimes(2);
  } finally {f.cleanup();}
});
it("does not consume an unrelated employee draft when a native status action succeeds", async () => {
  const f = directoryCommandFixture("directory"); try {
    f.current.hasDraft = true;
    f.send.mockResolvedValueOnce(directoryReply({id: "person", version: 2, status: "archived"}));
    const receipt = await f.commands.send("/person/status", {expectedVersion: 1, status: "archived"}); expect(receipt.unchanged()).toBe(false); receipt.acknowledge();
    await expect(f.commands.send("/another-person/status", {expectedVersion: 1, status: "active"})).rejects.toThrow("purchase or role");
    expect(f.send).toHaveBeenCalledTimes(1);
  } finally {f.cleanup();}
});

// Mounted native owner handlers use controlled hooks/transport; this is not browser acceptance.
import {canonicalUiHref as directoryCanonical} from "@erpx/feature-work_projection/lib/destination-registry";
type DirectoryElement = directoryReact.ReactElement<Record<string, unknown>>;
function mountDirectoryOwner(kind: "directory" | "editor" | "lifecycle", initial: Record<string, unknown>, send: typeof directoryFetch, query = new URLSearchParams()) {
  const require = directoryRequire(import.meta.url), ts = require("typescript") as typeof import("typescript"), values: unknown[] = [], effects = new Map<number, {dependencies?: unknown[]; cleanup?: () => void}>(), cache = new Map<string, Record<string, unknown>>();
  let cursor = 0, props = initial;
  const router = {push: vi.fn(), replace: vi.fn(), refresh: vi.fn()};
  const useState = (initial: unknown) => {const index = cursor++; if (!(index in values)) values[index] = typeof initial === "function" ? initial() : initial; return [values[index], (next: unknown) => {values[index] = typeof next === "function" ? next(values[index]) : next;}];};
  const useRef = (initial: unknown) => {const [ref] = useState(() => ({current: initial})); return ref;};
  const effect = (run: () => void | (() => void), dependencies?: unknown[]) => {const index = cursor++, old = effects.get(index); if (!old || !dependencies || dependencies.some((value, at) => value !== old.dependencies?.[at])) {old?.cleanup?.(); effects.set(index, {dependencies, cleanup: run() || undefined});}};
  const primitive = (name: string) => Object.assign((props: Record<string, unknown>) => directoryReact.createElement(({Button: "button", Field: "label", TextInput: "input", Select: "select", TextArea: "textarea", FormLayout: "form", Panel: "section", UiActionLink: "a", ActionLink: "a"} as Record<string, string>)[name] ?? "div", props, props.children as directoryReact.ReactNode), {displayName: name});
  const controls = new Proxy<Record<string, unknown>>({}, {get: (_target, name) => primitive(String(name))});
  const drawer = (props: Record<string, unknown>) => props.open || props.inline || props.retainFormContents ? directoryReact.createElement("aside", props, props.children as directoryReact.ReactNode) : null;
  const original = process.env.ERPX_DIRECTORY_BASELINE === "1";
  const sourcePath = kind === "directory" ? original ? "packages/feature-people_payroll/src/components/people-directory.tsx" : "packages/feature-people_payroll/src/components/people-directory-workspace.tsx" : kind === "editor" ? "packages/feature-people_payroll/src/components/people-editor.tsx" : original ? "packages/feature-people_payroll/src/components/lifecycle-workspace.tsx" : "packages/feature-people_payroll/src/components/directory-lifecycle-panel.tsx";
  const load = (path: string, baseline = false): Record<string, unknown> => {
    if (cache.has(path)) return cache.get(path)!;
    const root = baseline ? process.env.ERPX_DIRECTORY_BASELINE_SOURCE_ROOT : process.env.ERPX_DIRECTORY_SOURCE_ROOT;
    const source = directoryRead(root ? `${root}/${path}` : path, "utf8"), code = ts.transpileModule(source, {compilerOptions: {esModuleInterop: false, jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
    const evaluated = {exports: {} as Record<string, unknown>}; cache.set(path, evaluated.exports);
    const localRequire = (name: string): unknown => {
      if (name === "react") return {...directoryReact, useState, useRef, useEffect: effect, useLayoutEffect: effect};
      if (name === "react/jsx-runtime") return require(name);
      if (name.endsWith("@erpx/ui/components/foundation")) return controls;
      if (name.endsWith("@erpx/feature-people_payroll/lib/directory-owner-command")) return {createDirectoryOwnerCommand: (family: "directory" | "lifecycle") => createDirectoryOwnerCommand(family, send)};
      if (name.endsWith("@erpx/feature-people_payroll/lib/use-directory-owner-command")) return load("packages/feature-people_payroll/src/lib/use-directory-owner-command.ts");
      if (name.endsWith("@erpx/feature-work_projection/lib/work-fetch")) return {workFetch: send};
      if (name.endsWith("@erpx/feature-work_projection/lib/context-switch")) return directoryContext;
      if (name.endsWith("@erpx/feature-shared/lib/destination-router")) return {useRouter: () => router};
      if (name.endsWith("@erpx/feature-shared/lib/destination-search")) return {useSearchParams: () => query};
      if (name.endsWith("@erpx/feature-work_projection/lib/destination-registry")) return {canonicalUiHref: directoryCanonical};
      if (name.endsWith("/purchase-controls")) return directoryPurchase;
      if (name.endsWith("/people-lifecycle")) return {hasLifecycleCaseDetail: directoryHasDetail};
      if (name.endsWith("@erpx/feature-people_payroll/lib/retirement")) return directoryRetirement;
      if (name.endsWith("@erpx/ui/components/use-hydrated")) return {useHydrated: () => true};
      if (name.endsWith("@erpx/feature-people_payroll/components/people-form-drawer")) return {PeopleFormDrawer: drawer};
      if (name.endsWith("@erpx/ui/components/dialog")) return {Dialog: (props: Record<string, unknown>) => {
        const visited = useRef(false) as {current: boolean}; if (props.open) visited.current = true;
        return props.open || props.retainFormContents && props.presentation === "form-drawer" && visited.current ? drawer(props) : null;
      }};
      if (name.endsWith("@erpx/feature-people_payroll/components/gpo-status")) return {positionGpoStatus: () => "No access package", personGpoStatus: () => "No access package"};
      if (name.endsWith(".module.css")) return {default: new Proxy({}, {get: (_target, name) => String(name)})};
      if (name === "next/dynamic") return {default: () => primitive("LegacyLifecycle")};
      if (name === "lucide-react") return new Proxy({}, {get: () => () => null});
      if (name.endsWith("@erpx/workplace/components/workspace-link")) return {default: primitive("DestinationLink")};
      return new Proxy({}, {get: () => primitive("Child")});
    };
    directoryEvaluate(code, {require: localRequire, exports: evaluated.exports, module: evaluated, crypto, window, document, FormData, CustomEvent, URL, URLSearchParams, WeakRef, setTimeout, clearTimeout});
    return evaluated.exports;
  };
  const exported = load(sourcePath, original);
  const component = exported[kind === "directory" ? original ? "PeopleDirectory" : "PeopleDirectoryWorkspace" : kind === "editor" ? "PeopleEditor" : original ? "LifecyclePanel" : "DirectoryLifecyclePanel"] as (props: Record<string, unknown>) => directoryReact.ReactNode;
  const collect = (value: directoryReact.ReactNode, result: DirectoryElement[] = []): DirectoryElement[] => {directoryReact.Children.forEach(value, child => {if (!directoryReact.isValidElement<Record<string, unknown>>(child)) return; result.push(child); if (typeof child.type === "function") collect((child.type as (props: Record<string, unknown>) => directoryReact.ReactNode)(child.props), result); else collect(child.props.children as directoryReact.ReactNode, result);}); return result;};
  return {router, within: (node: DirectoryElement) => collect(node.props.children as directoryReact.ReactNode), render: () => {cursor = 0; return collect(component(props));}, props: (next: Record<string, unknown>) => {props = next;}, getProps: () => props, close: () => effects.forEach(item => item.cleanup?.())};
}
const directoryRow = {id: "person", personId: "person", userId: "account", engagementId: "engagement", positionId: null, managerPersonId: null, name: "Employee", email: "employee@example.test", jobTitle: "Coordinator", status: "active"};
it("preserves archived Directory query prefixes, repeated filters and cursor reset with standard controls and no mount read", () => {
  const browser = directoryBrowser(), send = vi.fn<typeof directoryFetch>(), component = mountDirectoryOwner("directory", {base: "/holdings/acme", rows: [directoryRow], lifecycle: null, queryPrefix: "archived."}, send, new URLSearchParams("filter=one&filter=two&archived.q=old&archived.cursor=cursor"));
  try {
    const nodes = component.render(); expect(send).not.toHaveBeenCalled();
    const filter = nodes.find(node => node.type === "select" && node.props["aria-label"] === "Retirement status")!;
    expect(filter.props.appearance).toBe("standard"); (filter.props.onChange as (event: unknown) => void)({target: {value: "retired"}});
    const url = new URL(String(component.router.push.mock.calls[0][0]), "https://people.example.test");
    expect(url.searchParams.getAll("filter")).toEqual(["one", "two"]); expect(url.searchParams.get("archived.q")).toBe("old");
    expect(url.searchParams.get("archived.retirement")).toBe("retired"); expect(url.searchParams.has("archived.cursor")).toBe(false);
    const row = nodes.find(node => node.type === "button" && node.props.draggable !== undefined)!;
    expect(row.props.appearance).toBe("standard"); expect(row.props.disabled).toBe(false);
  } finally {component.close(); browser.cleanup();}
});
it("scopes Directory dirty dismissal to its own employee drawer and never discards another mounted People form", () => {
  const browser = directoryBrowser(), unrelated = {id: "another-form"}, own = {id: "own-form"};
  vi.stubGlobal("document", {querySelector: (selector: string) => selector === "[data-work-revision]" ? browser.node : unrelated, activeElement: null});
  const component = mountDirectoryOwner("directory", {base: "/holdings/acme", rows: [directoryRow], lifecycle: null, queryPrefix: "archived."}, vi.fn<typeof directoryFetch>(), new URLSearchParams("archived.person=person"));
  try {
    let dirty = false, nodes = component.render();
    const body = nodes.find(node => node.type === "div" && node.props.className === "detail")!;
    const ref = body.props.ref as {current: unknown}; if (ref) ref.current = {querySelector: (selector: string) => selector.includes("data-work-dirty") && dirty ? own : null, querySelectorAll: () => [own]};
    const back = nodes.find(node => node.props.children === "← Back to people")!;
    (back.props.onClick as () => void)(); expect(component.router.push).toHaveBeenCalledTimes(1);
    component.router.push.mockClear(); dirty = true; (back.props.onClick as () => void)(); nodes = component.render();
    const keep = nodes.find(node => node.props.children === "Keep editing")!; expect(keep).toBeDefined(); (keep.props.onClick as () => void)(); expect(component.router.push).not.toHaveBeenCalled();
    (back.props.onClick as () => void)(); nodes = component.render(); (nodes.find(node => node.props.children === "Discard changes")!.props.onClick as () => void)();
    const discarded = browser.events.filter(event => event.type === "erpx:record-saved") as CustomEvent[];
    expect(discarded.map(event => event.detail.root)).toEqual([own]); expect(discarded.some(event => event.detail.root === unrelated)).toBe(false);
  } finally {component.close(); browser.cleanup();}
});
function mountedDirectoryEditor(send: typeof directoryFetch) {
  const fields = new Map<string, string>([["legalName", "Employee"], ["email", "employee@example.test"], ["reason", "Normal owner profile change"], ["workerType", "employee"], ["employmentType", "salaried"], ["startDate", "2026-01-01"], ["currency", "USD"]]);
  vi.stubGlobal("FormData", class {get(name: string) {return fields.get(name) ?? null;} getAll(name: string) {return fields.has(name) ? [fields.get(name)!] : [];}});
  const component = mountDirectoryOwner("editor", {appearance: "standard", commandScope: {ownerKey: "person", source: "native-ssr-1", canManage: true}, base: "/holdings/acme", person: {id: "person", engagement_id: "engagement", version: 1, engagement_version: 1, legal_name: "Employee", status: "active", engagement_status: "active"}, nodes: [], alwaysOpen: true, lifecycleManaged: true, purchases: lifecycleControls(true, true), returnTo: "/holdings/acme/people-organization/people/directory/current"}, send);
  const version = {value: "original"}, form = {isConnected: true, getAttribute: () => version.value} as unknown as HTMLFormElement;
  return {...component, fields, form, version};
}
it("does not navigate or clear the mounted standard employee editor when Work changes during ORIGINAL response JSON", async () => {
  const browser = directoryBrowser(), held = directoryDeferred<void>(), send = vi.fn<typeof directoryFetch>(async () => directoryReply({id: "person", version: 2}, () => held.promise)), component = mountedDirectoryEditor(send);
  try {
    const form = component.render().find(node => node.type === "form")!; (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick();
    browser.node.dataset.workCompany = "different-company"; held.resolve(); await directoryTick();
    expect(component.router.push).not.toHaveBeenCalled(); expect(component.router.refresh).not.toHaveBeenCalled();
    expect(component.render().some(node => node.props.children === "Recover original employee request")).toBe(true);
    expect(directoryContext.hasUnsavedWork()).toBe(true);
  } finally {component.close(); browser.cleanup();}
});
it("refreshes owner versions while retaining a newer mounted employee form and submits its current-version continuation", async () => {
  const browser = directoryBrowser(), held = directoryDeferred<void>(), send = vi.fn<typeof directoryFetch>().mockImplementationOnce(async () => directoryReply({id: "person", version: 2}, () => held.promise)).mockResolvedValueOnce(directoryReply({id: "person", version: 3})), component = mountedDirectoryEditor(send);
  try {
    let form = component.render().find(node => node.type === "form")!; (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick();
    component.fields.set("legalName", "Newer employee form"); component.version.value = "newer"; (form.props.onChangeCapture as () => void)(); component.render(); held.resolve(); await directoryTick();
    expect(component.router.push).not.toHaveBeenCalled(); expect(component.router.refresh).toHaveBeenCalledTimes(1);
    form = component.render().find(node => node.type === "form")!; expect(form).toBeDefined();
    const saved = browser.events.find(event => event.type === "erpx:record-command-saved") as CustomEvent;
    expect(saved.detail).toEqual({form: component.form, formEditVersion: "original"});
    expect(component.render().some(node => node.props.children === "The submitted version was saved. Your newer draft is retained.")).toBe(true);
    // Model the actual RSC result while retaining this mounted editor and its newer form values.
    component.props({...component.getProps(), commandScope: {ownerKey: "person", source: "native-ssr-version-2", canManage: true}, person: {id: "person", engagement_id: "engagement", version: 2, engagement_version: 2, legal_name: "Employee", status: "active", engagement_status: "active"}});
    form = component.render().find(node => node.type === "form")!;
    expect(component.fields.get("legalName")).toBe("Newer employee form"); expect(component.version.value).toBe("newer");
    expect(component.router.push).not.toHaveBeenCalled();
    (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick();
    expect(send).toHaveBeenCalledTimes(2);
    const first = JSON.parse(String(send.mock.calls[0][1]?.body)), next = JSON.parse(String(send.mock.calls[1][1]?.body));
    expect(first).toMatchObject({expectedVersion: 1, expectedEngagementVersion: 1, legalName: "Employee"});
    expect(next).toMatchObject({expectedVersion: 2, expectedEngagementVersion: 2, legalName: "Newer employee form"});
    expect(new Headers(send.mock.calls[1][1]?.headers).get("Idempotency-Key")).not.toBe(new Headers(send.mock.calls[0][1]?.headers).get("Idempotency-Key"));
    expect(component.router.push).toHaveBeenCalledExactlyOnceWith("/holdings/acme/people-organization/people/directory/current");
    expect(component.router.refresh).toHaveBeenCalledTimes(2);
    const acknowledgements = browser.events.filter(event => event.type === "erpx:record-command-saved") as CustomEvent[];
    expect(acknowledgements.map(event => event.detail.formEditVersion)).toEqual(["original", "newer"]);
  } finally {component.close(); browser.cleanup();}
});
it("recovers the mounted employee command's original body/key under refreshed history while retaining a newer form", async () => {
  const browser = directoryBrowser(), send = vi.fn<typeof directoryFetch>().mockRejectedValueOnce(Error("Native result lost")).mockResolvedValue(directoryReply({id: "person", version: 2})), component = mountedDirectoryEditor(send);
  try {
    let form = component.render().find(node => node.type === "form")!; (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick();
    component.fields.set("legalName", "Newer employee form"); component.version.value = "newer-form"; (form.props.onChangeCapture as () => void)(); form = component.render().find(node => node.type === "form")!; (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick(); expect(send).toHaveBeenCalledTimes(1);
    component.props({...component.getProps(), commandScope: {ownerKey: "person", source: "current-receipt-history", canManage: true}, purchases: lifecycleControls(false, true)});
    const recover = component.render().find(node => node.props.children === "Recover original employee request")!; (recover.props.onClick as () => void)(); await directoryTick();
    expect(send.mock.calls[1][1]?.body).toBe(send.mock.calls[0][1]?.body); expect(new Headers(send.mock.calls[1][1]?.headers).get("Idempotency-Key")).toBe(new Headers(send.mock.calls[0][1]?.headers).get("Idempotency-Key"));
    expect(component.router.push).not.toHaveBeenCalled();
    expect(component.render().some(node => node.props.children === "The submitted version was saved. Your newer draft is retained.")).toBe(true);
  } finally {component.close(); browser.cleanup();}
});
it("preserves standard lifecycle incoming/departure/history controls and opens genuine form drawers without a mount read", () => {
  const browser = directoryBrowser(), send = vi.fn<typeof directoryFetch>();
  const data: PeopleLifecycleData = {purchases: lifecycleControls(false, true), roster: [], managers: [], nodes: [], packages: [], positions: [], cases: [], tasks: [], settings: {version: 0, itAssigneeId: null, onboardingTasks: [], offboardingTasks: []}, canManage: true, canPrepare: false, canReview: true};
  const component = mountDirectoryOwner("lifecycle", {base: "/holdings/acme", data, person: {id: "person", engagementId: "engagement", positionId: null, managerPersonId: null, name: "Employee", status: "active"}}, send);
  try {
    let nodes = component.render(); expect(nodes.some(node => node.props.children === "Start a change")).toBe(false); expect(send).not.toHaveBeenCalled();
    const retire = nodes.find(node => node.props.children === "Retire employee")!; expect(retire.props.appearance).toBe("standard"); (retire.props.onClick as () => void)(); nodes = component.render();
    const drawer = nodes.find(node => node.type === "aside" && node.props.title === "Employee · employment change")!; expect(drawer.props.appearance).toBe("standard"); expect(drawer.props.open).toBe(true);
    const controls = nodes.filter(node => typeof node.type === "function" && ["Button", "Field", "Select", "TextInput", "TextArea"].includes((node.type as {displayName?: string}).displayName ?? "")); expect(controls.every(node => node.props.appearance === "standard")).toBe(true);
  } finally {component.close(); browser.cleanup();}
});

it("completes a normal mounted standard employee save once after its native identity/version acknowledgement", async () => {
  const browser = directoryBrowser(), send = vi.fn<typeof directoryFetch>(async () => directoryReply({id: "person", version: 2})), component = mountedDirectoryEditor(send);
  try {
    const form = component.render().find(node => node.type === "form")!; (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick();
    expect(component.router.push).toHaveBeenCalledExactlyOnceWith("/holdings/acme/people-organization/people/directory/current");
    expect(component.router.refresh).toHaveBeenCalledTimes(1);
    expect(component.render().some(node => node.props.children === "Recover original employee request")).toBe(false);
  } finally {component.close(); browser.cleanup();}
});
it("keeps cancellation of an actual retained departure enabled while incoming work drains, independent of the new editor's kind", () => {
  const browser = directoryBrowser(), send = vi.fn<typeof directoryFetch>();
  const definition = directoryLifecycleInput.parse({personId: "person", engagementId: "engagement", kind: "offboarding", positionId: null, effectiveAt: 1792296000000, expectedVersion: 0, reason: "Retained scheduled departure"});
  const data: PeopleLifecycleData = {purchases: lifecycleControls(false, true), roster: [], managers: [], nodes: [], packages: [], positions: [], cases: [{id: "case", company_id: "company", person_id: "person", engagement_id: "engagement", kind: "offboarding", status: "scheduled", version: 1, effective_at: definition.effectiveAt, package_id: null, package_revision: null, definition, created_by: "hr", updated_at: 1792209600000, workflow: "placement"}], tasks: [], settings: {version: 0, itAssigneeId: null, onboardingTasks: [], offboardingTasks: []}, canManage: true, canPrepare: false, canReview: false};
  const component = mountDirectoryOwner("lifecycle", {base: "/holdings/acme", data, person: {id: "person", engagementId: "engagement", positionId: null, managerPersonId: null, name: "Employee", status: "active"}}, send);
  try {
    const cancel = component.render().find(node => node.props.children === "Cancel scheduled change")!;
    expect(cancel).toBeDefined(); expect(cancel.props.disabled).toBe(false); expect(send).not.toHaveBeenCalled();
  } finally {component.close(); browser.cleanup();}
});
it("permits only the actual newly created Person/native key and same verified member to recover a profile-completion receipt", async () => {
  const f = directoryCommandFixture("directory"); try {
    f.current.ownerKey = "member:account"; f.current.recordId = undefined; f.current.memberUserId = "account";
    f.send.mockRejectedValueOnce(Error("Profile completion response lost"));
    await expect(f.commands.send("", {linkedMemberUserId: "account", expectedVersion: 0, legalName: "Employee"})).rejects.toThrow("response lost");
    const key = new Headers(f.send.mock.calls[0][1]?.headers).get("Idempotency-Key")!;
    f.current.recordId = `person-${key}`; f.current.source = "actual-linked-person-history";
    f.send.mockResolvedValueOnce(directoryReply({id: `person-${key}`, version: 1}));
    expect((await f.commands.retry()).acknowledge()).toBe(true);
    expect(f.send.mock.calls[1][1]?.body).toBe(f.send.mock.calls[0][1]?.body);
  } finally {f.cleanup();}
});

it.each(["rehire", "placement"] as const)("keeps pending %s recovery inside its actual lifecycle drawer and prevents dismissal before exact retry", async kind => {
  const browser = directoryBrowser(), fields = new Map<string, string>([["position", "position"], ["startDate", "2026-11-01"], ["reason", "Normal owner employment request"]]);
  vi.stubGlobal("FormData", class {get(name: string) {return fields.get(name) ?? null;} getAll(name: string) {return fields.has(name) ? [fields.get(name)!] : [];}});
  let mutations = 0;
  const send = vi.fn<typeof directoryFetch>(async (url, init) => {
    if (String(url).endsWith("/preview")) return directoryReply(directoryPreview);
    if (++mutations === 1) throw Error("Native employment result lost");
    const key = new Headers(init?.headers).get("Idempotency-Key"), input = JSON.parse(String(init?.body));
    return directoryReply(kind === "rehire" ? {id: `eng-rehire-${key}`, personId: "person", status: "onboarding"} : {id: input.id ?? `lifecycle-${key}`, version: Number(input.expectedVersion ?? 0) + 1, status: "scheduled"});
  });
  const data: PeopleLifecycleData = {purchases: lifecycleControls(true, true), roster: [], managers: [], nodes: [], packages: [], positions: [], cases: [], tasks: [], settings: {version: 0, itAssigneeId: null, onboardingTasks: [], offboardingTasks: []}, canManage: true, canPrepare: false, canReview: true};
  const component = mountDirectoryOwner("lifecycle", {base: "/holdings/acme", data, person: {id: "person", engagementId: "engagement", positionId: null, managerPersonId: null, name: "Employee", status: kind === "rehire" ? "ended" : "active"}}, send);
  const title = kind === "rehire" ? "Rehire · new engagement" : "Employee · employment change", form = {isConnected: true, getAttribute: () => "original"} as unknown as HTMLFormElement;
  try {
    let nodes = component.render(); (nodes.find(node => node.props.children === (kind === "rehire" ? "Create a new engagement for rehire" : "Start a change"))!.props.onClick as () => void)();
    nodes = component.render(); let drawer = nodes.find(node => node.type === "aside" && node.props.title === title)!;
    const requestForm = component.within(drawer).find(node => node.type === "form")!;
    (requestForm.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: form}); await directoryTick();
    if (kind === "placement") {
      drawer = component.render().find(node => node.type === "aside" && node.props.title === title)!;
      (component.within(drawer).find(node => node.props.children === "Submit for scheduled processing")!.props.onClick as () => void)(); await directoryTick();
    }
    nodes = component.render(); drawer = nodes.find(node => node.type === "aside" && node.props.title === title)!;
    expect(drawer.props.open).toBe(true); expect(drawer.props.dismissDisabled).toBe(true);
    expect(directoryContext.hasUnsavedWork()).toBe(true);
    const retry = component.within(drawer).find(node => node.props.children === "Recover original employment request")!;
    expect(retry).toBeDefined(); expect(retry.props.disabled).toBe(false);
    // Even a direct close callback cannot bypass the pending drawer's dismissal guard.
    (drawer.props.onClose as () => void)(); drawer = component.render().find(node => node.type === "aside" && node.props.title === title)!; expect(drawer.props.open).toBe(true);
    const originalIndex = kind === "rehire" ? 0 : 1, originalBody = send.mock.calls[originalIndex][1]?.body, originalKey = new Headers(send.mock.calls[originalIndex][1]?.headers).get("Idempotency-Key");
    (component.within(drawer).find(node => node.props.children === "Recover original employment request")!.props.onClick as () => void)(); await directoryTick();
    const recovered = send.mock.calls[originalIndex + 1][1];
    expect(recovered?.body).toBe(originalBody); expect(new Headers(recovered?.headers).get("Idempotency-Key")).toBe(originalKey);
    expect(component.router.refresh).toHaveBeenCalledTimes(1); expect(component.router.push).not.toHaveBeenCalled();
    expect(component.render().some(node => node.props["data-directory-command-pending"] === "true")).toBe(false);
    expect(directoryContext.hasUnsavedWork()).toBe(false);
  } finally {component.close(); browser.cleanup();}
});

// The actual record loader chooses the owner; existing legacy consumers retain their original panel.
import {DirectoryLifecyclePanel as standardRecordLifecyclePanel} from "@erpx/feature-people_payroll/components/directory-lifecycle-panel";
async function actualPersonRecordLifecyclePanel() {
  const require = directoryRequire(import.meta.url), ts = require("typescript") as typeof import("typescript"), captured: {load?: () => Promise<typeof standardRecordLifecyclePanel>} = {};
  const path = "packages/feature-records/src/components/loaders/people--lifecycle-workspace.tsx", root = process.env.ERPX_DIRECTORY_SOURCE_ROOT;
  const source = directoryRead(root ? `${root}/${path}` : path, "utf8"), code = ts.transpileModule(source, {compilerOptions: {esModuleInterop: false, jsx: ts.JsxEmit.ReactJSX, module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022}}).outputText;
  const evaluated = {exports: {} as Record<string, unknown>};
  directoryEvaluate(code, {exports: evaluated.exports, module: evaluated, require(name: string) {
    if (name === "next/dynamic") return {default: (load: () => Promise<typeof standardRecordLifecyclePanel>) => {captured.load = load; return () => null;}};
    if (name === "@erpx/feature-people_payroll/components/directory-lifecycle-panel") return {DirectoryLifecyclePanel: standardRecordLifecyclePanel};
    if (name === "@erpx/feature-people_payroll/components/lifecycle-workspace") return {LifecyclePanel};
    throw Error(`Unexpected Person record loader dependency: ${name}`);
  }});
  if (!captured.load || !evaluated.exports.LifecyclePanel) throw Error("The actual Person record lifecycle loader is incomplete.");
  return captured.load();
}
function personRecordDeparture() {
  const definition = directoryLifecycleInput.parse({personId: "person", engagementId: "engagement", kind: "offboarding", positionId: null, effectiveAt: 1792296000000, expectedVersion: 0, reason: "Actual retained scheduled departure"});
  const data: PeopleLifecycleData = {purchases: lifecycleControls(false, true), roster: [], managers: [], nodes: [], packages: [], positions: [], cases: [{id: "record-departure", company_id: "company", person_id: "person", engagement_id: "engagement", kind: "offboarding", status: "scheduled", version: 1, effective_at: definition.effectiveAt, package_id: null, package_revision: null, definition, created_by: "hr", updated_at: 1792209600000, workflow: "placement"}], tasks: [], settings: {version: 0, itAssigneeId: null, onboardingTasks: [], offboardingTasks: []}, canManage: true, canPrepare: false, canReview: false};
  return {base: "/holdings/acme", data, person: {id: "person", engagementId: "engagement", positionId: null, managerPersonId: null, name: "Actual record employee", status: "active"}};
}
it("renders the actual Person record loader with its standard lifecycle owner while preserving original legacy consumer presentation", async () => {
  const owner = await actualPersonRecordLifecyclePanel(), props = personRecordDeparture();
  expect(owner).toBe(standardRecordLifecyclePanel);
  const html = renderToStaticMarkup(createElement(owner, props));
  expect(html).toContain('<section data-slot="panel" data-appearance="standard"'); expect(html).toContain("Actual retained scheduled departure");
  expect(html).toContain("Cancel scheduled change"); expect(html).not.toContain("Start a change");
  // The owner migration does not rebind original Organization/Technology consumers.
  expect(renderLifecycle(lifecycleControls(false, true))).toContain('class="card"');
});
it("permits the Person record's draining departure cancel and recovers the identical original request under fresh terminal history", async () => {
  const owner = await actualPersonRecordLifecyclePanel(); expect(owner).toBe(standardRecordLifecyclePanel);
  const browser = directoryBrowser(), props = personRecordDeparture(), send = vi.fn<typeof directoryFetch>().mockRejectedValueOnce(Error("Native cancellation result lost")).mockResolvedValueOnce(directoryReply({id: "record-departure", status: "cancelled"}));
  const component = mountDirectoryOwner("lifecycle", props, send);
  try {
    let nodes = component.render(); const cancel = nodes.find(node => node.props.children === "Cancel scheduled change")!;
    expect(cancel.props.appearance).toBe("standard"); expect(cancel.props.disabled).toBe(false);
    (cancel.props.onClick as () => void)(); await directoryTick();
    expect(send).toHaveBeenCalledTimes(1); expect(String(send.mock.calls[0][0])).toBe("/api/v1/holdings/acme/people/lifecycle/cases/record-departure/cancel");
    expect(JSON.parse(String(send.mock.calls[0][1]?.body))).toEqual({expectedVersion: 1});
    expect(directoryContext.hasUnsavedWork()).toBe(true);
    component.props({...props, data: {...props.data, cases: props.data.cases.map(item => ({...item, status: "cancelled", version: 2}))}});
    nodes = component.render(); expect(nodes.some(node => node.props.children === "Cancel scheduled change")).toBe(false);
    const recover = nodes.find(node => node.props.children === "Recover original employment request")!;
    expect(recover).toBeDefined(); (recover.props.onClick as () => void)(); await directoryTick();
    expect(send).toHaveBeenCalledTimes(2); expect(send.mock.calls[1][1]?.body).toBe(send.mock.calls[0][1]?.body);
    expect(new Headers(send.mock.calls[1][1]?.headers).get("Idempotency-Key")).toBe(new Headers(send.mock.calls[0][1]?.headers).get("Idempotency-Key"));
    expect(component.router.refresh).toHaveBeenCalledTimes(1); expect(component.router.push).not.toHaveBeenCalled();
    expect(component.render().some(node => node.props["data-directory-command-pending"] === "true")).toBe(false);
    expect(directoryContext.hasUnsavedWork()).toBe(false);
  } finally {component.close(); browser.cleanup();}
});
// A first typed error is not rollback evidence after executeCommand loses its
// batch/COMMIT acknowledgement and then checks current replay authority.
it.each([
  {family: "directory" as const, status: 403}, {family: "directory" as const, status: 409},
  {family: "lifecycle" as const, status: 403}, {family: "lifecycle" as const, status: 409},
])("retains the first typed $status $family mutation and recovers its exact native request under fresh history", async ({family, status}) => {
  const f = directoryCommandFixture(family), edit = {value: "submitted"}, form = {isConnected: true, getAttribute: () => edit.value} as unknown as HTMLFormElement;
  const path = family === "directory" ? "/person" : "placements/submit", input = family === "directory" ? {expectedVersion: 1, legalName: "Original employee", reason: "Reviewed native change"} : f.input;
  try {
    f.send.mockResolvedValueOnce(directoryReply({code: status === 403 ? "FORBIDDEN" : "WORK_ROLE_UNAVAILABLE", message: "Current replay authority is unavailable"}, undefined, status));
    await expect(f.commands.send(path, input, {form, formEditVersion: edit.value})).rejects.toThrow("Current replay authority is unavailable");
    expect(f.commands.hasPending()).toBe(true); expect(f.events).toEqual([]);
    await expect(f.commands.send(path, {...input, reason: "A newer unsent draft"})).rejects.toThrow("Recover the original"); expect(f.send).toHaveBeenCalledTimes(1);
    // Revocation cannot discard the retained attempt or allow a replacement.
    f.current.canManage = false; f.current.source = "current-denied-owner";
    await expect(f.commands.retry()).rejects.toThrow("purchase or role"); expect(f.commands.hasPending()).toBe(true); expect(f.send).toHaveBeenCalledTimes(1);
    f.current.canManage = true; f.current.source = "current-native-history"; f.current.purchases = lifecycleControls(false, true); f.current.draftVersion = 2; edit.value = "newer";
    const recovered = await f.commands.retry(); expect(recovered.unchanged()).toBe(false); expect(f.events).toEqual([]);
    // A source change even after helper return must still retain the original.
    f.current.source = "later-native-history"; expect(() => recovered.acknowledge()).toThrow("owner"); expect(f.commands.hasPending()).toBe(true); expect(f.events).toEqual([]);
    const current = await f.commands.retry(); expect(current.unchanged()).toBe(false); expect(current.acknowledge()).toBe(true); expect(current.acknowledge()).toBe(false); expect(f.commands.hasPending()).toBe(false);
    const first = f.send.mock.calls[0]; expect(f.send.mock.calls.every(call => call[1]?.body === first[1]?.body && new Headers(call[1]?.headers).get("Idempotency-Key") === new Headers(first[1]?.headers).get("Idempotency-Key") && call[3] === "caller")).toBe(true);
    expect(f.send.mock.calls.slice(1).map(call => call[2])).toEqual([{form, formEditVersion: "submitted"}, {form, formEditVersion: "submitted"}]);
    const completions = f.events.filter(event => event.type === "erpx:record-command-saved") as CustomEvent[];
    expect(completions).toHaveLength(1); expect(completions[0].detail).toEqual({form, formEditVersion: "submitted"}); expect(edit.value).toBe("newer");
  } finally {f.cleanup();}
});

it.each([403, 409])("mounted employee recovery remains visible after a first typed %s and current authority loss without consuming a newer form", async status => {
  const browser = directoryBrowser(), send = vi.fn<typeof directoryFetch>().mockResolvedValueOnce(directoryReply({code: status === 403 ? "FORBIDDEN" : "WORK_ROLE_UNAVAILABLE", message: "Current replay authority is unavailable"}, undefined, status)).mockResolvedValueOnce(directoryReply({id: "person", version: 2})), component = mountedDirectoryEditor(send);
  try {
    let nodes = component.render(), form = nodes.find(node => node.type === "form")!;
    (form.props.onSubmit as (event: unknown) => void)({preventDefault() {}, currentTarget: component.form}); await directoryTick();
    expect(send).toHaveBeenCalledTimes(1); expect(component.router.push).not.toHaveBeenCalled(); expect(component.router.refresh).not.toHaveBeenCalled(); expect(browser.events).toEqual([]);
    component.props({...component.getProps(), commandScope: {ownerKey: "person", source: "native-current-revoked", canManage: false}});
    nodes = component.render(); let recover = nodes.find(node => node.props.children === "Recover original employee request")!; expect(recover).toBeDefined(); expect(recover.props.disabled).toBe(false);
    (recover.props.onClick as () => void)(); await directoryTick(); expect(send).toHaveBeenCalledTimes(1); expect(component.render().some(node => node.props.children === "Recover original employee request")).toBe(true);
    component.fields.set("legalName", "Newer retained employee form"); component.version.value = "newer"; form = component.render().find(node => node.type === "form")!; (form.props.onChangeCapture as () => void)();
    component.props({...component.getProps(), commandScope: {ownerKey: "person", source: "native-current-history", canManage: true}, purchases: lifecycleControls(false, true)});
    nodes = component.render(); recover = nodes.find(node => node.props.children === "Recover original employee request")!; (recover.props.onClick as () => void)(); await directoryTick();
    expect(send).toHaveBeenCalledTimes(2); expect(send.mock.calls[1][1]?.body).toBe(send.mock.calls[0][1]?.body); expect(new Headers(send.mock.calls[1][1]?.headers).get("Idempotency-Key")).toBe(new Headers(send.mock.calls[0][1]?.headers).get("Idempotency-Key"));
    expect(component.router.push).not.toHaveBeenCalled(); expect(component.router.refresh).toHaveBeenCalledTimes(1); expect(component.fields.get("legalName")).toBe("Newer retained employee form"); expect(component.version.value).toBe("newer");
    expect(component.render().some(node => node.props.children === "The submitted version was saved. Your newer draft is retained.")).toBe(true);
    const completions = browser.events.filter(event => event.type === "erpx:record-command-saved") as CustomEvent[]; expect(completions).toHaveLength(1); expect(completions[0].detail).toEqual({form: component.form, formEditVersion: "original"});
  } finally {component.close(); browser.cleanup();}
});


it('keeps the actual guarded employee form mounted through retained close, role loss, reopen and exact pending retry',async()=>{
 const browser=directoryBrowser(),send=vi.fn<typeof directoryFetch>().mockRejectedValueOnce(Error('Original employee result unknown')).mockResolvedValueOnce(directoryReply({id:'person',version:2})),component=mountedDirectoryEditor(send);
 component.props({...component.getProps(),alwaysOpen:false,initialOpen:true});
 try{
  let nodes=component.render(),drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Edit employee')!;const form=component.within(drawer).find(node=>node.type==='form')!;
  (form.props.onSubmit as (event:unknown)=>void)({preventDefault(){},currentTarget:component.form});await directoryTick();
  component.fields.set('legalName','Newer retained employee');component.version.value='newer';(form.props.onChangeCapture as ()=>void)();component.props({...component.getProps(),commandScope:{ownerKey:'person',source:'current-denied-owner',canManage:false}});nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Edit employee')!;
  expect(drawer.props.dismissDisabled).toBe(true);expect(drawer.props.retainFormContents).toBe(true);(drawer.props.onClose as ()=>void)();expect(component.render().find(node=>node.type==='aside'&&node.props.title==='Edit employee')!.props.open).toBe(true);
  (drawer.props.onDiscard as ()=>void)();expect(component.render().find(node=>node.type==='aside'&&node.props.title==='Edit employee')!.props.open).toBe(true);expect(send).toHaveBeenCalledTimes(1);
  const close=component.within(drawer).find(node=>node.props.children==='Close and retain request')!;expect(close.props.disabled).toBe(false);(close.props.onClick as ()=>void)();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Edit employee')!;expect(drawer.props.open).toBe(false);expect(component.within(drawer).some(node=>node.type==='form')).toBe(true);expect(component.fields.get('legalName')).toBe('Newer retained employee');expect(directoryContext.hasUnsavedWork()).toBe(true);
  const reopen=nodes.find(node=>node.props.children==='Reopen retained employee form')!;(reopen.props.onClick as ()=>void)();expect(component.render().find(node=>node.type==='aside'&&node.props.title==='Edit employee')!.props.open).toBe(true);
  component.props({...component.getProps(),commandScope:{ownerKey:'person',source:'fresh-retained-native-history',canManage:true},purchases:lifecycleControls(false,true)});nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Edit employee')!;(component.within(drawer).find(node=>node.props.children==='Recover original employee request')!.props.onClick as ()=>void)();await directoryTick();
  expect(send).toHaveBeenCalledTimes(2);expect(send.mock.calls[1][1]?.body).toBe(send.mock.calls[0][1]?.body);expect(new Headers(send.mock.calls[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(send.mock.calls[0][1]?.headers).get('Idempotency-Key'));expect(component.router.push).not.toHaveBeenCalled();expect(component.fields.get('legalName')).toBe('Newer retained employee');expect(component.render().some(node=>node.props.children==='The submitted version was saved. Your newer draft is retained.')).toBe(true);
 }finally{component.close();browser.cleanup();}
});
it.each(['rehire','placement'] as const)('retains a real %s lifecycle drawer through revoked authority and exact history retry without permitting discard',async kind=>{
 const browser=directoryBrowser(),fields=new Map([['position','position'],['startDate','2026-11-01'],['reason','Original retained employment reason']]);vi.stubGlobal('FormData',class{get(name:string){return fields.get(name)??null;}getAll(name:string){return fields.has(name)?[fields.get(name)!]:[];}});
 let mutations=0;const send=vi.fn<typeof directoryFetch>(async(url,init)=>{if(String(url).endsWith('/preview'))return directoryReply(directoryPreview);if(++mutations===1)throw Error('Native lifecycle result unknown');const id=new Headers(init?.headers).get('Idempotency-Key'),input=JSON.parse(String(init?.body));return directoryReply(kind==='rehire'?{id:`eng-rehire-${id}`,personId:'person',status:'onboarding'}:{id:input.id??`lifecycle-${id}`,version:Number(input.expectedVersion??0)+1,status:'scheduled'});});
 const data:PeopleLifecycleData={purchases:lifecycleControls(true,true),roster:[],managers:[],nodes:[],packages:[],positions:[],cases:[],tasks:[],settings:{version:0,itAssigneeId:null,onboardingTasks:[],offboardingTasks:[]},canManage:true,canPrepare:false,canReview:true},props={base:'/holdings/acme',data,person:{id:'person',engagementId:'engagement',positionId:null,managerPersonId:null,name:'Employee',status:kind==='rehire'?'ended':'active'}},component=mountDirectoryOwner('lifecycle',props,send),title=kind==='rehire'?'Rehire · new engagement':'Employee · employment change',form={isConnected:true,getAttribute:()=> 'original'} as unknown as HTMLFormElement;
 try{
  let nodes=component.render();(nodes.find(node=>node.props.children===(kind==='rehire'?'Create a new engagement for rehire':'Start a change'))!.props.onClick as ()=>void)();nodes=component.render();let drawer=nodes.find(node=>node.type==='aside'&&node.props.title===title)!;(component.within(drawer).find(node=>node.type==='form')!.props.onSubmit as (event:unknown)=>void)({preventDefault(){},currentTarget:form});await directoryTick();
  if(kind==='placement'){drawer=component.render().find(node=>node.type==='aside'&&node.props.title===title)!;(component.within(drawer).find(node=>node.props.children==='Submit for scheduled processing')!.props.onClick as ()=>void)();await directoryTick();}
  const index=kind==='rehire'?0:1,first=send.mock.calls[index][1];component.props({...props,data:{...data,canManage:false,canReview:false}});nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title===title)!;expect(drawer.props.retainFormContents).toBe(true);expect(drawer.props.dismissDisabled).toBe(true);(drawer.props.onClose as ()=>void)();expect(component.render().find(node=>node.type==='aside'&&node.props.title===title)!.props.open).toBe(true);
  (component.within(drawer).find(node=>node.props.children==='Close and retain request')!.props.onClick as ()=>void)();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title===title)!;expect(drawer.props.open).toBe(false);expect(directoryContext.hasUnsavedWork()).toBe(true);expect(nodes.some(node=>node.props.children==='Recover original employment request')).toBe(true);expect(fields.get('reason')).toBe('Original retained employment reason');
  (nodes.find(node=>node.props.children==='Reopen retained employment form')!.props.onClick as ()=>void)();expect(component.render().find(node=>node.type==='aside'&&node.props.title===title)!.props.open).toBe(true);
  component.props({...props,data:{...data,purchases:lifecycleControls(false,true)}});nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title===title)!;(component.within(drawer).find(node=>node.props.children==='Recover original employment request')!.props.onClick as ()=>void)();await directoryTick();expect(send.mock.calls[index+1][1]?.body).toBe(first?.body);expect(new Headers(send.mock.calls[index+1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(first?.headers).get('Idempotency-Key'));expect(component.router.push).not.toHaveBeenCalled();expect(component.router.refresh).toHaveBeenCalledTimes(1);
 }finally{component.close();browser.cleanup();}
});
it('the actual outer People details drawer retains its selected owner and nested lifecycle while a pending request is explicitly closed',()=>{
 const browser=directoryBrowser(),component=mountDirectoryOwner('directory',{base:'/holdings/acme',rows:[directoryRow],lifecycle:personRecordDeparture().data,canManage:true,editor:{person:{id:'person',version:1},nodes:[],levels:[],people:[],organization:{positions:[],people:[],nodes:[],levels:[]},levelManagedSeparately:false}},vi.fn<typeof directoryFetch>(),new URLSearchParams('person=person&filter=one&filter=two'));
 try{
  let nodes=component.render();const detail=nodes.find(node=>node.type==='div'&&node.props.className==='detail')!,ref=detail.props.ref as {current:unknown};let busy=true;ref.current={querySelector:(selector:string)=>selector.includes('command-busy')?busy?{}:null:selector.includes('command-pending')?{}:null,querySelectorAll:()=>[]};
  const editor=nodes.find(node=>typeof node.props.onCloseRetained==='function'&&node.props.alwaysOpen===true)!;expect(editor).toBeDefined();(editor.props.onCloseRetained as ()=>void)();nodes=component.render();expect(nodes.some(node=>node.props.children==='Reopen retained person details')).toBe(false);expect(component.router.push).not.toHaveBeenCalled();busy=false;(editor.props.onCloseRetained as ()=>void)();nodes=component.render();const outer=nodes.find(node=>typeof node.props.title==='string'&&node.props.presentation==='form-drawer')!;expect(outer.props.open).toBe(false);expect(outer.props.retainFormContents).toBe(true);const retained=component.within(outer),sameEditor=retained.find(node=>node.props.alwaysOpen===true)!;expect(sameEditor.key).toBe(editor.key);expect(sameEditor.props.commandScope).toEqual(editor.props.commandScope);expect(retained.find(node=>node.props.parentOpen!==undefined)!.props.parentOpen).toBe(false);expect(component.router.push).not.toHaveBeenCalled();
  (nodes.find(node=>node.props.children==='Reopen retained person details')!.props.onClick as ()=>void)();nodes=component.render();expect(nodes.find(node=>node.props.presentation==='form-drawer')!.props.open).toBe(true);expect(component.router.push).not.toHaveBeenCalled();
 }finally{component.close();browser.cleanup();}
});


it.each(['people','disclosure'] as const)('the actual %s drawer wrapper refuses a previously opened discard confirmation once its owner becomes pending',kind=>{
 const browser=directoryBrowser(),require=directoryRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),root=process.env.ERPX_DIRECTORY_SOURCE_ROOT??'.',path=kind==='people'?'packages/feature-people_payroll/src/components/people-form-drawer.tsx':'packages/ui/src/components/form-disclosure.tsx',source=directoryRead(`${root}/${path}`,'utf8'),code=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,evaluated={exports:{} as Record<string,unknown>},discard=vi.fn(),close=vi.fn(),Draft=(props:Record<string,unknown>)=>directoryReact.createElement('aside',props,props.children as directoryReact.ReactNode),values:unknown[]=[];let cursor=0;
 const useState=(initial:unknown)=>{const at=cursor++;if(!(at in values))values[at]=typeof initial==='function'?initial():initial;return [values[at],(next:unknown)=>{values[at]=typeof next==='function'?next(values[at]):next;}];};
 try{
  const localRequire=(name:string):unknown=>{if(name==='react')return {...directoryReact,useState,useRef:(value:unknown)=>({current:value}),useEffect:()=>{},useSyncExternalStore:(_subscribe:unknown,read:()=>unknown)=>read()};if(name==='react/jsx-runtime')return require(name);if(name.endsWith("@erpx/ui/components/draft-form-drawer"))return {DraftFormDrawer:Draft};if(name.endsWith("@erpx/ui/components/dialog"))return {Dialog:Draft};if(name.endsWith("@erpx/feature-work_projection/lib/context-switch"))return directoryContext;if(name.endsWith("@erpx/ui/lib/cn"))return {cn:(...items:unknown[])=>items.filter(Boolean).join(' ')};if(name.endsWith("@erpx/ui/components/foundation"))return {InlineNotice:Draft};throw Error('Unexpected actual retained drawer import '+name);};
  directoryEvaluate(code,{require:localRequire,exports:evaluated.exports,module:evaluated,window,document,crypto,CustomEvent,Set});const Component=evaluated.exports[kind==='people'?'PeopleFormDrawer':'FormDisclosure'] as (props:Record<string,unknown>)=>directoryReact.ReactNode,props={appearance:'standard',open:true,title:'Actual owner draft',drawerTitle:'Actual owner draft',retainFormContents:true,dismissDisabled:true,onDiscard:discard,onClose:close,children:directoryReact.createElement('form')};
  const collect=(value:directoryReact.ReactNode,result:DirectoryElement[]=[]):DirectoryElement[]=>{directoryReact.Children.forEach(value,child=>{if(!directoryReact.isValidElement<Record<string,unknown>>(child))return;result.push(child);collect(child.props.children as directoryReact.ReactNode,result);});return result;};
  cursor=0;let drawer=collect(Component(props)).find(node=>node.type===Draft)!;expect(drawer.props.retainFormContents).toBe(true);(drawer.props.onDiscard as ()=>void)();expect(discard).not.toHaveBeenCalled();expect(close).not.toHaveBeenCalled();expect(browser.events.filter(event=>event.type==='erpx:record-saved')).toEqual([]);
  cursor=0;drawer=collect(Component({...props,dismissDisabled:false})).find(node=>node.type===Draft)!;(drawer.props.onDiscard as ()=>void)();expect(discard).toHaveBeenCalledTimes(1);expect(close).not.toHaveBeenCalled();
 }finally{browser.cleanup();}
});


it('retained People details reject another row or explorer placement before any optimistic owner replacement, then release lawful navigation',()=>{
 const browser=directoryBrowser(),first=directoryRow,second={...directoryRow,id:'second',personId:'second',engagementId:'second-engagement',name:'Second employee'},data=personRecordDeparture().data;data.canManage=true;data.purchases=lifecycleControls(true,true);data.positions=[{id:'target',title:'Actual target position',version:1,requirements:'',package_id:null,package_revision:null,gpo:{mapped:false,directoryStatus:null},branch_id:null,department_id:null,level_id:'level',active:1}];
 const component=mountDirectoryOwner('directory',{base:'/holdings/acme',rows:[first,second],hasExplorer:true,lifecycle:data,canManage:true,editor:{person:{id:'person',version:1},nodes:[],levels:[],people:[],organization:{positions:[],people:[],nodes:[],levels:[]},levelManagedSeparately:false}},vi.fn<typeof directoryFetch>(),new URLSearchParams('person=person&display=explorer&filter=one&filter=two'));
 try{
  let nodes=component.render();const body=nodes.find(node=>node.type==='div'&&node.props.className==='detail')!,ref=body.props.ref as {current:unknown};let pending=true,dirty=false;
  ref.current={querySelector:(selector:string)=>selector.includes('data-directory-command-pending')&&pending?{}:selector.includes('data-work-dirty')&&dirty?{}:null,querySelectorAll:()=>[]};const editor=nodes.find(node=>node.props.alwaysOpen===true)!,lifecycleKey=nodes.find(node=>node.props.parentOpen!==undefined)!.key;(editor.props.onCloseRetained as ()=>void)();nodes=component.render();expect(nodes.some(node=>node.props.children==='Reopen retained person details')).toBe(true);
  const row=()=>component.render().find(node=>node.key==='second'&&typeof node.props.onClick==='function')!,target=()=>component.render().find(node=>node.type==='div'&&typeof node.props.onDrop==='function')!;
  (row().props.onClick as ()=>void)();expect(component.router.push).not.toHaveBeenCalled();expect(component.render().find(node=>node.props.alwaysOpen===true)!.key).toBe(editor.key);
  (target().props.onDrop as (event:unknown)=>void)({preventDefault(){},dataTransfer:{getData:()=> 'second'}});expect(component.router.push).not.toHaveBeenCalled();expect(component.render().find(node=>node.props.parentOpen!==undefined)!.key).toBe(lifecycleKey);
  pending=false;dirty=true;(row().props.onClick as ()=>void)();expect(component.router.push).not.toHaveBeenCalled();expect(component.render().some(node=>node.props.children==='Reopen retained person details')).toBe(true);
  dirty=false;(row().props.onClick as ()=>void)();expect(component.router.push).toHaveBeenCalledTimes(1);const destination=new URL(String(component.router.push.mock.calls[0][0]),'https://people.example.test');expect(destination.searchParams.get('person')).toBe('second');expect(destination.searchParams.getAll('filter')).toEqual(['one','two']);
 }finally{component.close();browser.cleanup();}
});


function lifecyclePanelOwner(nodes:DirectoryElement[]){const panel=nodes.find(node=>node.props['data-directory-command-pending']!==undefined||node.props.onChangeCapture!==undefined&&node.props.appearance==='standard'&&node.props['data-directory-lifecycle-draft']===undefined);if(!panel)throw Error('Actual Lifecycle Panel owner missing');return panel;}
function lifecyclePortalForm(owner:'change'|'rehire',reset:()=>void=()=>{}){let version='original';const form={dataset:{directoryLifecycleOwner:owner},isConnected:true,getAttribute:()=>version,setAttribute:(_name:string,value:string)=>{version=value;},reset:vi.fn(reset)} as unknown as HTMLFormElement;return {form,newer:()=>{version='newer';},target:{closest:()=>form}};}
it('portal-backed newer lifecycle edits remain owner-dirty after the older native ACK and block hidden-parent Person and placement replacement until explicit discard',async()=>{
 const browser=directoryBrowser(),fields=new Map([['position','position'],['startDate','2026-11-01'],['reason','Original portal employment reason']]);vi.stubGlobal('FormData',class{get(name:string){return fields.get(name)??null;}getAll(name:string){return fields.has(name)?[fields.get(name)!]:[];}});let mutations=0;
 const send=vi.fn<typeof directoryFetch>(async(url,init)=>{if(String(url).endsWith('/preview'))return directoryReply(directoryPreview);if(++mutations===1)throw Error('Original portal mutation unknown');const key=new Headers(init?.headers).get('Idempotency-Key');return directoryReply({id:`lifecycle-${key}`,version:1,status:'scheduled'});}),data:PeopleLifecycleData={purchases:lifecycleControls(true,true),roster:[],managers:[],nodes:[],packages:[],positions:[],cases:[],tasks:[],settings:{version:0,itAssigneeId:null,onboardingTasks:[],offboardingTasks:[]},canManage:true,canPrepare:false,canReview:true},props={base:'/holdings/acme',data,person:{id:'person',engagementId:'engagement',positionId:null,managerPersonId:null,name:'Employee',status:'active'}},child=mountDirectoryOwner('lifecycle',props,send),portal=lifecyclePortalForm('change',()=>{fields.set('reason','Original portal employment reason');});
 const parentData={...data,positions:[{id:'target',title:'Actual target',version:1,requirements:'',package_id:null,package_revision:null,gpo:{mapped:false,directoryStatus:null},branch_id:null,department_id:null,level_id:'level',active:1}]},parent=mountDirectoryOwner('directory',{base:'/holdings/acme',rows:[directoryRow,{...directoryRow,id:'second',personId:'second',engagementId:'second-engagement',name:'Second employee'}],hasExplorer:true,lifecycle:parentData,canManage:true,editor:{person:{id:'person',version:1},nodes:[],levels:[],people:[],organization:{positions:[],people:[],nodes:[],levels:[]},levelManagedSeparately:false}},vi.fn<typeof directoryFetch>(),new URLSearchParams('person=person&display=explorer&filter=one&filter=two'));
 try{
  let nodes=child.render();(nodes.find(node=>node.props.children==='Start a change')!.props.onClick as ()=>void)();nodes=child.render();let drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;
  (lifecyclePanelOwner(nodes).props.onChangeCapture as (event:unknown)=>void)({target:portal.target});child.render();await (child.within(drawer).find(node=>node.type==='form')!.props.onSubmit as (event:unknown)=>Promise<void>)({preventDefault(){},currentTarget:portal.form});await directoryTick();nodes=child.render();expect(lifecyclePanelOwner(nodes).props['data-directory-owner-dirty']).toBe('true');drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;
  await (child.within(drawer).find(node=>node.props.children==='Submit for scheduled processing')!.props.onClick as ()=>Promise<void>)();await directoryTick();nodes=child.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;(child.within(drawer).find(node=>node.props.children==='Back')!.props.onClick as ()=>void)();nodes=child.render();fields.set('reason','Newer portal-backed employment draft');portal.newer();(lifecyclePanelOwner(nodes).props.onChangeCapture as (event:unknown)=>void)({target:portal.target});nodes=child.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;(child.within(drawer).find(node=>node.props.children==='Close and retain request')!.props.onClick as ()=>void)();
  child.props({...props,data:{...data,purchases:lifecycleControls(false,true)}});nodes=child.render();(nodes.find(node=>node.props.children==='Recover original employment request')!.props.onClick as ()=>void)();await directoryTick();nodes=child.render();const panel=lifecyclePanelOwner(nodes);expect(panel.props['data-directory-command-pending']).toBeUndefined();expect(panel.props['data-directory-owner-dirty']).toBe('true');expect(directoryContext.hasUnsavedWork()).toBe(true);expect(fields.get('reason')).toBe('Newer portal-backed employment draft');expect(send.mock.calls[2][1]?.body).toBe(send.mock.calls[1][1]?.body);expect(new Headers(send.mock.calls[2][1]?.headers).get('Idempotency-Key')).toBe(new Headers(send.mock.calls[1][1]?.headers).get('Idempotency-Key'));
  // The actual portal form is OUTSIDE this simulated parent DOM subtree. Only
  // the Lifecycle Panel's real owner marker is visible to detailRoot queries.
  let parentNodes=parent.render();const detail=parentNodes.find(node=>node.type==='div'&&node.props.className==='detail')!,ref=detail.props.ref as {current:unknown};ref.current={querySelector:(selector:string)=>{const current=lifecyclePanelOwner(child.render());if(selector.includes('data-directory-command-pending')&&current.props['data-directory-command-pending']==='true')return {};if(selector.includes('data-directory-command-busy')&&current.props['data-directory-command-busy']==='true')return {};if(selector.includes('data-directory-owner-dirty')&&current.props['data-directory-owner-dirty']==='true')return {};return null;},querySelectorAll:()=>[]};
  const editor=parentNodes.find(node=>node.props.alwaysOpen===true)!;(editor.props.onCloseRetained as ()=>void)();parentNodes=parent.render();expect(parentNodes.some(node=>node.props.children==='Reopen retained person details')).toBe(true);const row=()=>parent.render().find(node=>node.key==='second'&&typeof node.props.onClick==='function')!,target=()=>parent.render().find(node=>node.type==='div'&&typeof node.props.onDrop==='function')!;(row().props.onClick as ()=>void)();(target().props.onDrop as (event:unknown)=>void)({preventDefault(){},dataTransfer:{getData:()=> 'second'}});expect(parent.router.push).not.toHaveBeenCalled();expect(parent.render().find(node=>node.props.alwaysOpen===true)!.key).toBe(editor.key);
  (parentNodes.find(node=>node.props.children==='Reopen retained person details')!.props.onClick as ()=>void)();nodes=child.render();(nodes.find(node=>node.props.children==='Reopen retained employment form')!.props.onClick as ()=>void)();nodes=child.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;expect(drawer.props.open).toBe(true);expect(fields.get('reason')).toBe('Newer portal-backed employment draft');(drawer.props.onDiscard as ()=>void)();nodes=child.render();expect(lifecyclePanelOwner(nodes).props['data-directory-owner-dirty']).toBeUndefined();expect(portal.form.reset).toHaveBeenCalledTimes(1);expect(directoryContext.hasUnsavedWork()).toBe(false);(row().props.onClick as ()=>void)();expect(parent.router.push).toHaveBeenCalledTimes(1);
 }finally{parent.close();child.close();browser.cleanup();}
});
it('only a matching lifecycle mutation clears its owner-local dirty revision; readonly preview and an unrelated saved event do not',async()=>{
 const browser=directoryBrowser(),fields=new Map([['position','position'],['reason','Reviewed portal employment']]);vi.stubGlobal('FormData',class{get(name:string){return fields.get(name)??null;}getAll(name:string){return fields.has(name)?[fields.get(name)!]:[];}});const send=vi.fn<typeof directoryFetch>(async(url,init)=>String(url).endsWith('/preview')?directoryReply(directoryPreview):directoryReply({id:`lifecycle-${new Headers(init?.headers).get('Idempotency-Key')}`,version:1,status:'scheduled'})),data:PeopleLifecycleData={purchases:lifecycleControls(true,true),roster:[],managers:[],nodes:[],packages:[],positions:[],cases:[],tasks:[],settings:{version:0,itAssigneeId:null,onboardingTasks:[],offboardingTasks:[]},canManage:true,canPrepare:false,canReview:true},component=mountDirectoryOwner('lifecycle',{base:'/holdings/acme',data,person:{id:'person',engagementId:'engagement',positionId:null,managerPersonId:null,name:'Employee',status:'active'}},send),portal=lifecyclePortalForm('change');
 try{
  let nodes=component.render();(nodes.find(node=>node.props.children==='Start a change')!.props.onClick as ()=>void)();nodes=component.render();(lifecyclePanelOwner(nodes).props.onInputCapture as (event:unknown)=>void)({target:portal.target});nodes=component.render();expect(lifecyclePanelOwner(nodes).props['data-directory-owner-dirty']).toBe('true');window.dispatchEvent(new CustomEvent('erpx:record-command-saved',{detail:{form:{getAttribute:()=> 'original'},formEditVersion:'original'}}));expect(lifecyclePanelOwner(component.render()).props['data-directory-owner-dirty']).toBe('true');
  let drawer=component.render().find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;await (component.within(drawer).find(node=>node.type==='form')!.props.onSubmit as (event:unknown)=>Promise<void>)({preventDefault(){},currentTarget:portal.form});await directoryTick();nodes=component.render();expect(lifecyclePanelOwner(nodes).props['data-directory-owner-dirty']).toBe('true');drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Employee · employment change')!;await (component.within(drawer).find(node=>node.props.children==='Submit for scheduled processing')!.props.onClick as ()=>Promise<void>)();await directoryTick();expect(lifecyclePanelOwner(component.render()).props['data-directory-owner-dirty']).toBeUndefined();expect(directoryContext.hasUnsavedWork()).toBe(false);
 }finally{component.close();browser.cleanup();}
});


import {RosterOwnerWorkspace} from '@erpx/feature-people_payroll/components/roster-owner-workspace';
const renderRosterOwner=(initial:RosterData,appearance:'legacy'|'standard',initialView='team')=>renderToStaticMarkup(createElement(RosterOwnerWorkspace,{initial,appearance,base:'/holdings/acme/people-organization/people',embedded:true,initialView}));
describe('actual Roster owner presentation seam',()=>{
 it.each(['team','mine','templates','requests'])('preserves the complete original legacy %s markup and native controls',view=>{
  for(const purchase of [undefined,{newWork:true,settleExisting:true,history:true,revision:'active'},{newWork:false,settleExisting:true,history:true,revision:'draining'},{newWork:false,settleExisting:false,history:true,revision:'history'}]){
   const data=roster(purchase);expect(renderRosterOwner(data,'legacy',view)).toBe(renderRoster(data,view));
  }
 });
 it('renders the standard native seven-day grid from the owner SSR snapshot without HTTP or legacy control appearance',()=>{
  const network=vi.fn();vi.stubGlobal('fetch',network);
  try{
   const html=renderRosterOwner(roster(),'standard');
   expect(html).toContain('data-ui-appearance="standard"');expect(html).toContain('aria-label="Team weekly roster"');
   expect(html.match(/<section[^>]*><header>/g)).toHaveLength(7);expect(html.match(/data-testid="roster-shift"/g)).toHaveLength(1);
   expect(html).toContain('Morning');expect(html).toContain('06:00–14:00');expect(html).toContain('Previous week');expect(html).toContain('Next week');
   expect(html).toContain('data-appearance="standard"');expect(html).not.toContain('data-appearance="legacy"');expect(network).not.toHaveBeenCalled();
  }finally{vi.unstubAllGlobals();}
 });
 it('keeps standard draining cancellation and retained templates without allowing new planning',()=>{
  const data=roster({newWork:false,settleExisting:true,history:true,revision:'draining'}),html=renderRosterOwner(data,'standard');
  expect(html).toContain('Morning');expect(html).toContain('Cancel shift');
  for(const control of ['Add shift','Edit shift','Request slot','Request swap','Copy week','Create week'])expect(html).not.toContain(control);
  const templates=renderRosterOwner(data,'standard','templates');expect(templates).toContain('Morning template');expect(templates).not.toContain('New template');expect(templates).not.toContain('Edit template');
  const history=renderRosterOwner({...data,purchase:{...data.purchase!,settleExisting:false}},'standard');expect(history).toContain('Morning');expect(history).not.toContain('Cancel shift');
 });
 it('preserves request deep-link identity and permitted review actions in the standard owner',()=>{
  const data=roster();data.requests=[{id:'request-current',company_id:'company',department_id:'department',kind:'extra',state:'pending_manager',version:3,requester_person_id:'person',coworker_person_id:null,requester_name:'Original requester',coworker_name:null,source_assignment_id:null,source_version:null,target_shift_id:'shift',target_assignment_id:null,target_version:null,reason:'Original requested shift',decision_note:'',created_at:1,permittedActions:['approve','reject']}];
  const html=renderToStaticMarkup(createElement(RosterOwnerWorkspace,{initial:data,base:'/holdings/acme/people-organization/people',embedded:true,appearance:'standard',initialView:'requests',requestId:'request-current'}));
  expect(html).toContain('id="request-request-current"');expect(html).toContain('Original requester');expect(html).toContain('Original requested shift');expect(html).toContain('Approve change');expect(html).toContain('Reject');
 });
});

it('the actual new Roster owner retains its original key/body and newer draft when Work changes during original JSON',async()=>{
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),source=await readFile(new URL("../../../packages/feature-people_payroll/src/components/roster-owner-workspace.tsx",import.meta.url),'utf8'),file=ts.createSourceFile('actual-roster-owner-successor.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX),functions:import('typescript').FunctionDeclaration[]=[];
 const visit=(node:import('typescript').Node)=>{if(ts.isFunctionDeclaration(node)&&['send','apply'].includes(node.name?.text??''))functions.push(node);ts.forEachChild(node,visit);};visit(file);if(functions.length!==2)throw Error('Expected the actual Roster owner send and apply handlers.');
 const emitted=ts.transpileModule(functions.map(fn=>fn.getText(file)).join('\n'),{compilerOptions:{esModuleInterop: false, target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;
 const {announceWorkChange}=await import('@erpx/feature-work_projection/lib/work-refresh');
 const node={isConnected:true,dataset:{workUser:'operator',workRevision:'7',workCompany:'company',workAssignment:'assignment',workStale:'false'}},form={isConnected:true,getAttribute:()=> 'original-form-edit'},events:string[]=[];
 const browser=Object.assign(new EventTarget(),{location:{origin:'https://people-dev-erpx.jesadakorn.com'}});browser.addEventListener('erpx:work-changed',()=>events.push('saved'));
 vi.stubGlobal('window',browser);vi.stubGlobal('document',{querySelector:(selector:string)=>selector==='[data-work-revision]'?node:null,activeElement:{closest:()=>form},documentElement:{dataset:{}}});
 let release!:ReadableStreamDefaultController<Uint8Array>,signalBody!:()=>void;const bodyStarted=new Promise<void>(resolve=>{signalBody=resolve;}),body=new ReadableStream<Uint8Array>({start(controller){release=controller;}});
 class OriginalJson extends Response{override json(){signalBody();return super.json();}}
 let calls=0;const network=vi.fn<(url:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>(async()=>{if(++calls===1)throw Error('Original response lost');return calls===2?new OriginalJson(body,{headers:{'content-type':'application/json'}}):Response.json({data:{id:'template',version:2}});});vi.stubGlobal('fetch',network);
 const state:Record<string,unknown[]>={},setter=(key:string)=>(value:unknown)=>{(state[key]??=[]).push(value);},load=vi.fn(async()=>{}),data=roster(),input={action:'template.save',id:'template',expectedVersion:1,departmentId:'department',name:'Submitted template',startTime:'06:00',endTime:'14:00',capacity:2,active:true};
 // The real controller now owns recovery. Record actual outgoing bodies, then
 // exercise the actual apply/send handler pair with the same original assertions.
 const trackedSend:typeof workFetch=(url,init,completion,acknowledgement)=>{(state.retry??=[]).push({body:init?.body});return workFetch(url,init,completion,acknowledgement);},commands=createRosterOwnerCommands(trackedSend,()=> 'original-owner-key'),draft={current:commands};commands.bind(()=>({base:'/holdings/acme',data,initial:data,view:'templates'}));commands.open();
 const names=['commands','completedEditor','busy','refreshRequired','submittedEditor','editorRoot','announceWorkChange','editor','data','load','notifyDraft','setDirty','setBusy','setError','setMessage','setPending','setEditor','setEditorOpen','setCompletedEditor','setRefreshRequired','input'],run=new Function(...names,`${emitted}\nreturn commands.hasPending()?apply(commands.retry):send(input);`) as (...ports:unknown[])=>Promise<void>;
 const submit=()=>run(commands,false,false,null,{current:{kind:'template',template:data.templates[0]}},{current:{querySelector:()=>form}},announceWorkChange,{kind:'template',template:data.templates[0]},data,load,()=>{},...['dirty','busy','error','message','pending','editor','editorOpen','completedEditor','refreshRequired'].map(setter),input);
 try{
  await submit();expect(state.error.at(-1)).toBe('Original response lost');draft.current.changed();
  const pending=submit();await bodyStarted;node.dataset.workCompany='different-company';release.enqueue(new TextEncoder().encode(JSON.stringify({data:{id:'template',version:2}})));release.close();await pending;
  expect(events).toEqual([]);expect(load).not.toHaveBeenCalled();expect(draft.current.isDirty()).toBe(true);expect(state.retry.at(-1)).toMatchObject({body:JSON.stringify(input)});
  node.dataset.workCompany='company';await submit();expect(events).toEqual(['saved']);expect(load).toHaveBeenCalledOnce();expect(draft.current.isDirty()).toBe(true);expect(state.message.at(-1)).toContain('newer draft is retained');
  expect(network.mock.calls.map(call=>call[1]?.body)).toEqual([JSON.stringify(input),JSON.stringify(input),JSON.stringify(input)]);expect(network.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['original-owner-key','original-owner-key','original-owner-key']);
 }finally{vi.unstubAllGlobals();}
});


import {RosterShiftForm} from '@erpx/feature-people_payroll/components/roster-shift-form';
it.each(['legacy','standard'] as const)('keeps the actual %s ShiftForm dates, selected source and native form fields in its owned appearance',appearance=>{
 const data=roster(),submit=vi.fn(async()=>{}),cancelShift=vi.fn();
 const html=renderToStaticMarkup(createElement(RosterShiftForm,{appearance,editor:{kind:'shift',shift:data.shifts[0]},data,busy:false,submit,cancelShift}));
 expect(html).toContain(`data-ui-appearance="${appearance}"`);expect(html).toContain(`data-appearance="${appearance}"`);
 expect(html).toContain('name="date"');expect(html).toContain('value="2026-10-19"');expect(html).toContain('value="Morning"');expect(html).toContain('value="06:00"');expect(html).toContain('value="14:00"');expect(html).toContain('name="reason"');
 if(appearance==='standard')expect(html).not.toContain('data-appearance="legacy"');
 expect(submit).not.toHaveBeenCalled();expect(cancelShift).not.toHaveBeenCalled();
});


import {createRosterOwnerCommands,currentRosterOwnerDraft,rosterOwnerDraftHeld,rosterOwnerReadVersion,type RosterOwnerSource,type RosterOwnerDraftState} from '@erpx/feature-people_payroll/lib/roster-owner-command';
import * as rosterNativeRules from '@erpx/feature-people_payroll/lib/people-roster';
import * as rosterCalendarRules from '@erpx/feature-work_projection/lib/calendar';
import * as rosterCalendarTimeline from '@erpx/feature-work_projection/lib/calendar-timeline';
import * as rosterCalendarCommands from '@erpx/feature-work_projection/lib/calendar-command';
import * as rosterUiFormat from '@erpx/feature-shared/lib/ui-format';
function rosterOwnerBrowser(){const browser=directoryBrowser(),target=new EventTarget();Object.assign(document,{addEventListener:target.addEventListener.bind(target),removeEventListener:target.removeEventListener.bind(target),querySelectorAll:()=>[],documentElement:{dataset:{}},visibilityState:'hidden'});Object.assign(window,{history:{pushState:vi.fn(),replaceState:vi.fn()}});return browser;}
function rosterOwnerFixture(){const browser=rosterOwnerBrowser();let key=0,current:RosterOwnerSource={base:'/holdings/acme',data:roster(),initial:roster(),view:'templates'};const send=vi.fn<typeof directoryFetch>(),commands=createRosterOwnerCommands(send,()=>`owner-key-${++key}`);commands.bind(()=>current);commands.open();return {browser,send,commands,get current(){return current;},set current(next:RosterOwnerSource){current=next;},cleanup:()=>browser.cleanup()};}
const rosterTemplateInput={action:'template.save',departmentId:'department',name:'Original template',startTime:'06:00',endTime:'14:00',capacity:2,active:true};
it.each([403,409] as const)('retains an actual first %s Roster response, forbids body replacement and recovers the same original receipt',async status=>{
 const f=rosterOwnerFixture();try{
  f.send.mockResolvedValueOnce(directoryReply({code:status===403?'ROSTER_ACCESS_CHANGED':'WORK_ROLE_UNAVAILABLE',message:'Current authority denied'},undefined,status)).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-1',version:1}));
  await expect(f.commands.send(rosterTemplateInput,'2026-10-19')).rejects.toThrow('Current authority denied');expect(f.commands.hasPending()).toBe(true);
  await expect(f.commands.send({...rosterTemplateInput,name:'Newer draft'},'2026-10-19')).rejects.toThrow('Recover the original');expect(()=>f.commands.open()).toThrow('Recover the original');
  f.current={...f.current,data:roster({newWork:false,settleExisting:false,history:true,revision:'lawful-retained-history'})};
  const recovered=await f.commands.retry();expect(recovered.data).toEqual({id:'roster-owner-key-1',version:1});recovered.acknowledge();
  expect(f.send.mock.calls.map(call=>call[1]?.body)).toEqual([JSON.stringify(rosterTemplateInput),JSON.stringify(rosterTemplateInput)]);expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-1']);
 }finally{f.cleanup();}
});
it('releases only a first exact native template-time rollback, keeps an earlier unknown and leaves fields dirty',async()=>{
 const f=rosterOwnerFixture();try{
  f.send.mockResolvedValueOnce(directoryReply({code:'ROSTER_TIME',message:'Choose different times'},undefined,422)).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-2',version:1})).mockRejectedValueOnce(Error('Lost original result')).mockResolvedValueOnce(directoryReply({code:'ROSTER_TIME',message:'Same editable error after unknown'},undefined,422));
  const invalid={...rosterTemplateInput,endTime:'06:00'};
  await expect(f.commands.send(invalid,'2026-10-19')).rejects.toThrow('Choose different times');expect(f.commands.hasPending()).toBe(false);expect(f.commands.isDirty()).toBe(true);
  const corrected={...rosterTemplateInput,endTime:'15:00'},saved=await f.commands.send(corrected,'2026-10-19');saved.acknowledge();expect(f.commands.hasPending()).toBe(false);
  await expect(f.commands.send(invalid,'2026-10-19')).rejects.toThrow('Lost original result');await expect(f.commands.retry()).rejects.toThrow('Same editable error after unknown');expect(f.commands.hasPending()).toBe(true);
  expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2','owner-key-3','owner-key-3']);expect(f.send.mock.calls[3][1]?.body).toBe(f.send.mock.calls[2][1]?.body);
 }finally{f.cleanup();}
});
it('validates the actual week guard independently from the native shift receipt version',async()=>{
 const f=rosterOwnerFixture();try{
  f.current.data.week!.version=9;f.current.data.weeks[0].version=9;f.current.data.shifts[0].version=3;
  const input={action:'shift.save',weekId:'week',expectedVersion:9,id:'shift',date:'2026-10-19',name:'Morning',startTime:'06:00',endTime:'14:00',capacity:2,personIds:[],reason:'Reviewed shift change'};
  f.send.mockResolvedValueOnce(directoryReply({id:'shift',version:10})).mockResolvedValueOnce(directoryReply({id:'shift',version:4}));
  await expect(f.commands.send(input,'2026-10-19')).rejects.toThrow('acknowledgement is incomplete');expect(f.commands.hasPending()).toBe(true);const receipt=await f.commands.retry();expect(receipt.data.version).toBe(4);receipt.acknowledge();expect(f.commands.hasPending()).toBe(false);
  expect(f.send.mock.calls[1][1]?.body).toBe(JSON.stringify(input));
 }finally{f.cleanup();}
});
it.each(['foreign ID','wrong state','extra field'] as const)('does not ACK a Roster response with %s',async malformed=>{
 const f=rosterOwnerFixture();try{const value=malformed==='foreign ID'?{id:'other',version:1,state:'draft'}:malformed==='wrong state'?{id:'roster-owner-key-1',version:1,state:'published'}:{id:'roster-owner-key-1',version:1,state:'draft',other:true};f.send.mockResolvedValueOnce(directoryReply(value));await expect(f.commands.send({action:'week.create',departmentId:'department',weekStart:'2026-10-19',timezone:'Asia/Bangkok'},'2026-10-19')).rejects.toThrow('acknowledgement is incomplete');expect(f.commands.hasPending()).toBe(true);}finally{f.cleanup();}
});
it.each(['source','unmount'] as const)('retains original Roster receipt/body when %s changes DURING ORIGINAL JSON',async changed=>{
 const f=rosterOwnerFixture(),body=directoryDeferred<void>(),started=directoryDeferred<void>();try{
  f.send.mockResolvedValueOnce(directoryReply({id:'roster-owner-key-1',version:1},async()=>{started.resolve();await body.promise;})).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-1',version:1}));
  const original=f.current,attempt=f.commands.send(rosterTemplateInput,'2026-10-19');await started.promise;f.commands.changed();
  if(changed==='source')f.current={...f.current,data:{...f.current.data,asOf:2,purchase:{newWork:false,settleExisting:true,history:true,revision:'fresh-history'}}};else f.commands.bind(()=>null);
  body.resolve();await expect(attempt).rejects.toThrow('Roster source');expect(f.commands.hasPending()).toBe(true);expect(f.commands.isDirty()).toBe(true);
  f.current={...original,data:{...original.data,asOf:3,purchase:{newWork:false,settleExisting:true,history:true,revision:'fresh-history'}}};f.commands.bind(()=>f.current);
  const recovered=await f.commands.retry();expect(recovered.unchanged()).toBe(false);recovered.acknowledge();expect(f.commands.isDirty()).toBe(true);expect(f.send.mock.calls[1][1]?.body).toBe(f.send.mock.calls[0][1]?.body);expect(new Headers(f.send.mock.calls[1][1]?.headers).get('Idempotency-Key')).toBe('owner-key-1');
 }finally{f.cleanup();}
});

function mountRosterOwner(kind:'owner'|'team'|'calendar',initial:Record<string,unknown>,send:typeof directoryFetch,reloadIntegration?:(endpoint:string)=>Promise<void>){
 const require=directoryRequire(import.meta.url),ts=require('typescript') as typeof import('typescript'),values:unknown[]=[],effects=new Map<number,{dependencies?:unknown[];cleanup?:()=>void}>();let cursor=0,props=initial,rendering=false,rerender=false,key=0;
 const useState=(initial:unknown)=>{const index=cursor++;if(!(index in values))values[index]=typeof initial==='function'?initial():initial;return [values[index],(next:unknown)=>{const value=typeof next==='function'?next(values[index]):next;if(value!==values[index]){values[index]=value;if(rendering)rerender=true;}}];};
 const useRef=(initial:unknown)=>{const [ref]=useState(()=>({current:initial}));return ref;};
 const memo=(run:()=>unknown,dependencies?:unknown[])=>{const index=cursor++,old=values[index] as {dependencies?:unknown[];value:unknown}|undefined;if(!old||!dependencies||dependencies.some((value,at)=>value!==old.dependencies?.[at]))values[index]={dependencies,value:run()};return (values[index] as {value:unknown}).value;};
 const effect=(run:()=>void|(()=>void),dependencies?:unknown[])=>{const index=cursor++,old=effects.get(index);if(!old||!dependencies||dependencies.some((value,at)=>value!==old.dependencies?.[at])){old?.cleanup?.();effects.set(index,{dependencies,cleanup:run()||undefined});}};
 const primitive=(name:string)=>Object.assign((props:Record<string,unknown>)=>directoryReact.createElement(({Button:'button',Field:'label',TextInput:'input',Select:'select',TextArea:'textarea',FormLayout:'form'} as Record<string,string>)[name]??'div',props,props.children as directoryReact.ReactNode),{displayName:name});
 const controls=new Proxy<Record<string,unknown>>({},{get:(_target,name)=>primitive(String(name))}),drawer=(props:Record<string,unknown>)=>props.open||props.retainFormContents?directoryReact.createElement('aside',props,props.children as directoryReact.ReactNode):null;
 const path=kind==='owner'?'packages/feature-people_payroll/src/components/roster-owner-workspace.tsx':kind==='team'?'packages/feature-people_payroll/src/components/people-team-schedule.tsx':'packages/feature-work_projection/src/components/calendar/standard-calendar-workspace.tsx',root=process.env.ERPX_ROSTER_SOURCE_ROOT??'.',source=directoryRead(`${root}/${path}`,'utf8'),code=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,loadedModule={exports:{} as Record<string,unknown>};
 let integrationRoster:RosterData|null=roster();const calendar={query:rosterCalendarRules.calendarQuerySchema.parse({date:'2026-10-19',view:'week'}),from:'2026-10-19',to:'2026-10-26',items:[],nextCursor:null,sources:[],coverage:{shifts:0,assigned:0,open:0,warnings:0,partial:false},selfPersonId:null,canManageRoster:true,canSchedule:false,canAnnounce:false,teamDefault:true},hierarchy={nodes:[],people:[],nextCursor:null,selfPersonId:null};
 const localRequire=(name:string):unknown=>{
  if(name==='react')return {...directoryReact,useState,useRef,useEffect:effect,useLayoutEffect:effect,useId:()=>{const [id]=useState(()=>`mounted-roster-${++key}`);return id;},useCallback:(fn:unknown,deps?:unknown[])=>memo(()=>fn,deps),useMemo:memo};
  if(name==='react/jsx-runtime')return require(name);
  if(name.endsWith("@erpx/ui/components/foundation"))return controls;
  if(name.endsWith("@erpx/feature-people_payroll/lib/roster-owner-command"))return {createRosterOwnerCommands:()=>createRosterOwnerCommands(send,()=>`mounted-key-${++key}`),currentRosterOwnerDraft,rosterOwnerDraftHeld,rosterOwnerReadVersion};
  if(name.endsWith('/people-roster'))return rosterNativeRules;
  if(name.endsWith("@erpx/feature-people_payroll/lib/roster-editor-draft"))return {currentRosterSnapshot};
  if(name.endsWith("@erpx/feature-work_projection/lib/work-fetch"))return {workFetch:send};
  if(name.endsWith("@erpx/feature-work_projection/lib/command-context"))return {captureWorkCommandContext};
  if(name.endsWith("@erpx/feature-work_projection/lib/context-switch"))return directoryContext;
  if(name.endsWith("@erpx/feature-work_projection/lib/work-refresh"))return {announceWorkChange:()=>window.dispatchEvent(new Event('erpx:work-changed'))};
  if(name.endsWith("@erpx/feature-work_projection/lib/destination-registry"))return {canonicalUiHref:directoryCanonical};
  if(name.endsWith("@erpx/feature-people_payroll/components/roster-owner-workspace"))return {RosterOwnerWorkspace:primitive('RosterChild')};
  if(name.endsWith("@erpx/feature-people_payroll/components/people-integration-data"))return {usePeopleIntegrationData:(endpoint:string)=>({data:endpoint.includes('/people/roster')?integrationRoster:endpoint.includes('/hierarchy')?hierarchy:calendar,loading:false,error:'',reload:async()=>{await reloadIntegration?.(endpoint);}}),usePeopleIntegrationCommand:()=>({busy:false,error:'',message:'',send:async()=>null})};
  if(name.endsWith("@erpx/feature-work_projection/lib/calendar-command"))return {...rosterCalendarCommands,createCalendarCommands:()=>rosterCalendarCommands.createCalendarCommands(send)};
  if(name.endsWith("@erpx/feature-work_projection/lib/calendar-timeline"))return rosterCalendarTimeline;
  if(name.endsWith("@erpx/feature-shared/lib/ui-format"))return rosterUiFormat;
  if(name.endsWith("@erpx/feature-work_projection/lib/calendar"))return rosterCalendarRules;
  if(name.endsWith("@erpx/feature-shared/components/features/feature-page"))return {WidgetGrid:primitive('WidgetGrid'),FeatureWidget:primitive('FeatureWidget')};
  if(name.endsWith("@erpx/ui/components/dialog"))return {Dialog:drawer};
  if(name.endsWith("@erpx/ui/components/draft-form-drawer"))return {DraftFormDrawer:drawer};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/calendar-draft-boundary"))return {CalendarDraftBoundary:primitive('CalendarDraftBoundary')};
  if(name.endsWith("@erpx/feature-people_payroll/components/time/time-preference-context"))return {TimeText:primitive('TimeText')};
  if(name.endsWith("@erpx/ui/components/primitives"))return {StatusBadge:primitive('StatusBadge'),PageHeader:(props:Record<string,unknown>)=>directoryReact.createElement('header',props,props.action as directoryReact.ReactNode)};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/calendar-event"))return {CalendarEvent:primitive('CalendarEvent'),eventTime:()=>'',eventStyle:()=>({}),layerNames:{}};
  if(name.endsWith("@erpx/ui/components/form-disclosure"))return {FormDisclosure:drawer};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/standard-calendar-forms"))return {StandardCalendarForm:primitive('CalendarForm')};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/calendar-day-timeline"))return {CalendarDayTimeline:primitive('CalendarDayTimeline')};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/personal-time-grid"))return {PersonalTimeGrid:primitive('PersonalTimeGrid')};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/month-grid"))return {MonthGrid:primitive('MonthGrid')};
  if(name.endsWith("@erpx/feature-work_projection/components/calendar/month-navigation"))return {MonthNavigation:primitive('MonthNavigation')};
  if(name.endsWith("@erpx/feature-people_payroll/components/roster-shift-form"))return {RosterShiftForm:primitive('RosterShiftForm')};
  if(name.endsWith("@erpx/workplace/components/workspace-link"))return {default:primitive('WorkspaceLink')};
  if(name.endsWith('.module.css'))return {default:new Proxy({},{get:(_target,name)=>String(name)})};
  if(name==='lucide-react')return new Proxy({},{get:()=>()=>null});
  throw Error('Unexpected actual Roster import '+name);
 };
 directoryEvaluate(code,{require:localRequire,module:loadedModule,exports:loadedModule.exports,window,document,crypto,FormData,CustomEvent,Event,URL,URLSearchParams,WeakRef,Map,Set});
 const Component=loadedModule.exports[kind==='owner'?'RosterOwnerWorkspace':kind==='team'?'PeopleTeamSchedule':'StandardCalendarWorkspace'] as (props:Record<string,unknown>)=>directoryReact.ReactNode;
 const collect=(value:directoryReact.ReactNode,result:DirectoryElement[]=[]):DirectoryElement[]=>{directoryReact.Children.forEach(value,child=>{if(!directoryReact.isValidElement<Record<string,unknown>>(child))return;result.push(child);if(typeof child.type==='function')collect((child.type as (props:Record<string,unknown>)=>directoryReact.ReactNode)(child.props),result);else collect(child.props.children as directoryReact.ReactNode,result);});return result;};
 return {calendar,hierarchy,rosterResult:(next:RosterData|null)=>{integrationRoster=next;},props:(next:Record<string,unknown>)=>{props=next;},getProps:()=>props,within:(node:DirectoryElement)=>collect(node.props.children as directoryReact.ReactNode),render:()=>{let result:DirectoryElement[]=[];for(let attempts=0;attempts<6;attempts++){cursor=0;rerender=false;rendering=true;try{result=collect(Component(props));}finally{rendering=false;}if(!rerender)return result;}throw Error('Actual owner did not settle its state-derived render');},close:()=>effects.forEach(effect=>effect.cleanup?.())};
}
const rosterRenderedText=(value:directoryReact.ReactNode):string=>directoryReact.Children.toArray(value).map(child=>typeof child==='string'||typeof child==='number'?String(child):directoryReact.isValidElement<Record<string,unknown>>(child)?rosterRenderedText(child.props.children as directoryReact.ReactNode):'').join('');
const rosterNodeText=(nodes:DirectoryElement[],text:string)=>{const node=nodes.find(node=>rosterRenderedText(node.props.children as directoryReact.ReactNode).replace(/\s+/g,' ').trim()===text&&typeof node.props.onClick==='function');if(!node)throw Error('Actual Roster control missing: '+text);return node;};
it('mounted Roster unknown drawer retains the same form/key through role loss, protected discard, close/reopen and native history recovery',async()=>{
 const browser=rosterOwnerBrowser(),fields=new Map([['name','Original template'],['start','06:00'],['end','14:00'],['capacity','2'],['active','on']]);let version='original',post=0,originalKey='';
 vi.stubGlobal('FormData',class{get(name:string){return fields.get(name)??null;}getAll(name:string){return fields.has(name)?[fields.get(name)!]:[];}});
 const data=roster(),send=vi.fn<typeof directoryFetch>(async(_url,init)=>{if(init?.method!=='POST')return directoryReply({...data,asOf:3,templates:[...data.templates,{...data.templates[0],id:`roster-${originalKey}`,version:1}]});if(++post===1){originalKey=new Headers(init.headers).get('Idempotency-Key')!;throw Error('Original template unknown');}return directoryReply({id:`roster-${originalKey}`,version:1});}),changed=vi.fn(),component=mountRosterOwner('owner',{base:'/holdings/acme',initial:data,embedded:true,appearance:'standard',initialView:'templates',onDraftStateChange:changed},send),form={isConnected:true,getAttribute:()=>version,setAttribute:(_name:string,value:string)=>{version=value;},reset:vi.fn()};
 try{
  let nodes=component.render();(rosterNodeText(nodes,'New template').props.onClick as ()=>void)();nodes=component.render();let drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='New shift template')!,owner=component.within(drawer).find(node=>typeof node.props.onChangeCapture==='function')!;(owner.props.ref as {current:unknown}).current={querySelector:()=>form,contains:()=>true};
  (owner.props.onChangeCapture as (event:unknown)=>void)({target:{closest:()=>form}});const actualForm=component.within(drawer).find(node=>node.type==='form')!;await (actualForm.props.onSubmit as (event:unknown)=>void)({preventDefault(){},currentTarget:form});await directoryTick();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='New shift template')!;
  expect(drawer.props.dismissDisabled).toBe(true);expect(drawer.props.retainFormContents).toBe(true);expect(directoryContext.hasUnsavedWork()).toBe(true);(drawer.props.onDiscard as ()=>void)();expect(component.render().some(node=>node.type==='aside'&&node.props.title==='New shift template')).toBe(true);
  fields.set('name','Newer retained template');version='newer';owner=component.within(drawer).find(node=>typeof node.props.onChangeCapture==='function')!;(owner.props.onChangeCapture as (event:unknown)=>void)({target:{closest:()=>form}});
  component.props({...component.getProps(),initial:{...data,asOf:2,canManage:false,purchase:{newWork:false,settleExisting:false,history:true,revision:'retained'}}});nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='New shift template')!;
  (rosterNodeText(component.within(drawer),'Close and retain request').props.onClick as ()=>void)();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='New shift template')!;expect(drawer.props.open).toBe(false);expect(component.within(drawer).some(node=>node.type==='form')).toBe(true);expect(fields.get('name')).toBe('Newer retained template');expect(form.reset).not.toHaveBeenCalled();
  (rosterNodeText(nodes,'Reopen retained Roster form').props.onClick as ()=>void)();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='New shift template')!;(rosterNodeText(component.within(drawer),'Recover original Roster request').props.onClick as ()=>void)();await directoryTick();nodes=component.render();
  const calls=send.mock.calls.filter(call=>call[1]?.method==='POST');expect(calls).toHaveLength(2);expect(calls[1][1]?.body).toBe(calls[0][1]?.body);expect(new Headers(calls[1][1]?.headers).get('Idempotency-Key')).toBe(originalKey);expect(fields.get('name')).toBe('Newer retained template');expect(form.reset).not.toHaveBeenCalled();expect(nodes.some(node=>node.props.children==='Submitted change saved. Your newer draft is retained.')).toBe(true);expect(changed.mock.calls.at(-1)?.[0]).toMatchObject({dirty:true,pending:false});
 }finally{component.close();browser.cleanup();}
});
it('mounted TeamSchedule refuses a view change and retains its child key through forced newer props until its exact owner draft settles',()=>{
 const browser=rosterOwnerBrowser(),back=vi.fn(),component=mountRosterOwner('team',{base:'/holdings/acme',initialWeek:'2026-10-19',initialView:'team',onBack:back},vi.fn<typeof directoryFetch>());
 try{let nodes=component.render();const child=nodes.find(node=>typeof node.props.onDraftStateChange==='function')!,key=child.key;const report=child.props.onDraftStateChange as (state:RosterOwnerDraftState)=>void;report({ownerId:'child',revision:5,dirty:true,pending:true,busy:false});nodes=component.render();const select=nodes.find(node=>node.type==='select'&&node.props.value==='team')!;expect(select.props.disabled).toBe(true);(select.props.onChange as (event:unknown)=>void)({target:{value:'requests'}});expect(window.history.pushState).not.toHaveBeenCalled();component.props({...component.getProps(),initialView:'requests'});nodes=component.render();expect(nodes.find(node=>typeof node.props.onDraftStateChange==='function')!.key).toBe(key);component.rosterResult(null);nodes=component.render();expect(nodes.find(node=>typeof node.props.onDraftStateChange==='function')!.key).toBe(key);expect(nodes.find(node=>typeof node.props.onDraftStateChange==='function')!.props.ownerAvailable).toBe(false);report({ownerId:'child',revision:4,dirty:false,pending:false,busy:false});expect(component.render().find(node=>node.type==='select')!.props.disabled).toBe(true);component.rosterResult(roster());report({ownerId:'child',revision:6,dirty:false,pending:false,busy:false});nodes=component.render();expect(nodes.find(node=>node.type==='select')!.props.disabled).toBe(false);}finally{component.close();browser.cleanup();}
});
it('mounted Calendar keeps the actual Roster subtree through pending outer close and reopens it without a replacement read',async()=>{
 const browser=rosterOwnerBrowser(),send=vi.fn<typeof directoryFetch>().mockResolvedValue(directoryReply(roster())),dummy=mountRosterOwner('calendar',{},send),props={base:'/holdings/acme',initial:dummy.calendar,initialHierarchy:dummy.hierarchy,initialAccounts:[],contextKey:'calendar-company'},component=mountRosterOwner('calendar',props,send);
 try{let nodes=component.render();(rosterNodeText(nodes,'Plan roster').props.onClick as ()=>void)();await directoryTick();nodes=component.render();let outer=nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!;const child=component.within(outer).find(node=>typeof node.props.onDraftStateChange==='function')!,initial=child.props.initial;const report=child.props.onDraftStateChange as (state:RosterOwnerDraftState)=>void;report({ownerId:'calendar-child',revision:3,dirty:true,pending:true,busy:false});nodes=component.render();outer=nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!;(outer.props.onClose as ()=>void)();nodes=component.render();const confirmation=nodes.find(node=>node.type==='aside'&&node.props.title==='Retain Roster work?')!;expect(component.within(confirmation).some(node=>node.props.children==='Discard Roster draft')).toBe(false);(rosterNodeText(component.within(confirmation),'Close and retain Roster').props.onClick as ()=>void)();nodes=component.render();outer=nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!;expect(outer.props.open).toBe(false);expect(outer.props.retainFormContents).toBe(true);expect(component.within(outer).find(node=>typeof node.props.onDraftStateChange==='function')!.props.initial).toBe(initial);(rosterNodeText(nodes,'Reopen retained Roster planning').props.onClick as ()=>void)();nodes=component.render();expect(nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!.props.open).toBe(true);(rosterNodeText(nodes,'Plan roster').props.onClick as ()=>void)();await directoryTick();expect(send).toHaveBeenCalledTimes(1);report({ownerId:'calendar-child',revision:2,dirty:false,pending:false,busy:false});nodes=component.render();outer=nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!;(outer.props.onClose as ()=>void)();expect(component.render().some(node=>node.type==='aside'&&node.props.title==='Retain Roster work?')).toBe(true);}finally{component.close();browser.cleanup();}
});

it.each(['template.save','week.create','week.publish','shift.save','shift.cancel','request.create','request.decide'] as const)('accepts only the exact native %s ID/version/state contract',async action=>{
 const f=rosterOwnerFixture();try{
  const data=f.current.data;data.requests=[{id:'request',company_id:'company',department_id:'department',kind:'extra',requester_person_id:'person',coworker_person_id:null,requester_name:'Original requester',coworker_name:null,source_assignment_id:null,source_version:null,target_shift_id:'shift',target_assignment_id:null,target_version:null,state:'pending_manager',reason:'Actual request',decision_note:'',version:3,created_at:1,permittedActions:['reject']}];data.week!.state='draft';data.weeks[0].state='draft';
  const input=action==='template.save'?{...rosterTemplateInput,id:'template',expectedVersion:1}:action==='week.create'?{action,departmentId:'department',weekStart:'2026-10-19',timezone:'Asia/Bangkok'}:action==='week.publish'?{action,id:'week',expectedVersion:1}:action==='shift.save'?{action,id:'shift',weekId:'week',expectedVersion:1,date:'2026-10-19',name:'Shift',startTime:'06:00',endTime:'14:00',capacity:2,personIds:[],reason:'Current shift review'}:action==='shift.cancel'?{action,id:'shift',expectedVersion:1,reason:'Current cancellation'}:action==='request.create'?{action,kind:'extra',targetShiftId:'shift',reason:'Current staff request'}:{action,id:'request',expectedVersion:3,decision:'reject',reason:'Independent rejection'};
  const native=action==='template.save'?{id:'template',version:2}:action==='week.create'?{id:'roster-owner-key-1',version:1,state:'draft'}:action==='week.publish'?{id:'week',version:2,state:'published'}:action==='shift.save'?{id:'shift',version:2}:action==='shift.cancel'?{id:'shift',version:2,state:'cancelled'}:action==='request.create'?{id:'roster-owner-key-1',version:1,state:'pending_manager'}:{id:'request',version:4,state:'rejected'};
  f.send.mockResolvedValueOnce(directoryReply(native));const result=await f.commands.send(input,'2026-10-19');expect(result.data).toEqual(native);expect(f.commands.hasPending()).toBe(true);result.acknowledge();expect(f.commands.hasPending()).toBe(false);
 }finally{f.cleanup();}
});
it('mounted Calendar refuses a late replacement Roster read when its existing child becomes dirty during ORIGINAL JSON',async()=>{
 const browser=rosterOwnerBrowser(),started=directoryDeferred<void>(),release=directoryDeferred<void>(),send=vi.fn<typeof directoryFetch>().mockResolvedValueOnce(directoryReply(roster())).mockResolvedValueOnce(directoryReply({...roster(),asOf:9},async()=>{started.resolve();await release.promise;})),dummy=mountRosterOwner('calendar',{},send),component=mountRosterOwner('calendar',{base:'/holdings/acme',initial:dummy.calendar,initialHierarchy:dummy.hierarchy,initialAccounts:[],contextKey:'calendar-company'},send);
 try{let nodes=component.render();(rosterNodeText(nodes,'Plan roster').props.onClick as ()=>void)();await directoryTick();nodes=component.render();const outer=nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!;const child=component.within(outer).find(node=>typeof node.props.onDraftStateChange==='function')!,initial=child.props.initial;
  (rosterNodeText(nodes,'Plan roster').props.onClick as ()=>void)();await started.promise;(child.props.onDraftStateChange as (state:RosterOwnerDraftState)=>void)({ownerId:'current-child',revision:2,dirty:true,pending:false,busy:false});release.resolve();await directoryTick();nodes=component.render();const currentOuter=nodes.find(node=>node.type==='aside'&&node.props.title==='Roster planning')!;expect(component.within(currentOuter).find(node=>typeof node.props.onDraftStateChange==='function')!.props.initial).toBe(initial);expect(nodes.some(node=>node.props.children==='The retained Roster draft takes precedence over a late read.')).toBe(true);expect(send).toHaveBeenCalledTimes(2);
 }finally{component.close();browser.cleanup();}
});

it('keeps a retained Roster attempt when its current parent source is unavailable and sends nothing until that source returns',async()=>{
 const f=rosterOwnerFixture();try{f.send.mockRejectedValueOnce(Error('Original result unknown')).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-1',version:1}));await expect(f.commands.send(rosterTemplateInput,'2026-10-19')).rejects.toThrow('unknown');f.current={...f.current,available:false};await expect(f.commands.retry()).rejects.toThrow('Roster source');expect(f.commands.hasPending()).toBe(true);expect(f.send).toHaveBeenCalledTimes(1);f.current={...f.current,available:true,data:{...f.current.data,asOf:7}};const result=await f.commands.retry();result.acknowledge();expect(f.send.mock.calls[1][1]?.body).toBe(f.send.mock.calls[0][1]?.body);expect(f.commands.hasPending()).toBe(false);}finally{f.cleanup();}
});
it.each(['form-drawer','fullscreen'] as const)('the actual opt-in %s Dialog keeps its visited Roster form mounted after close and reopens it without replacing the child',presentation=>{
 const require=directoryRequire(import.meta.url),source=directoryRead(`${process.env.ERPX_ROSTER_SOURCE_ROOT??'.'}/packages/ui/src/components/dialog.tsx`,'utf8'),ts=require('typescript') as typeof import('typescript'),code=ts.transpileModule(source,{compilerOptions:{esModuleInterop: false, jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,loadedModule={exports:{} as Record<string,unknown>},values:unknown[]=[];let cursor=0;
 const useState=(initial:unknown)=>{const index=cursor++;if(!(index in values))values[index]=typeof initial==='function'?initial():initial;return [values[index],(value:unknown)=>{values[index]=typeof value==='function'?value(values[index]):value;}];},primitive=(name:string)=>(props:Record<string,unknown>)=>directoryReact.createElement(name,props,props.children as directoryReact.ReactNode),parts=new Proxy({},{get:(_target,name)=>primitive(`dialog-${String(name).toLowerCase()}`)});
 directoryEvaluate(code,{module:loadedModule,exports:loadedModule.exports,require:(name:string)=>{
  if(name==='react')return {...directoryReact,useContext:()=>false,useState,useRef:(initial:unknown)=>{const [ref]=useState(()=>({current:initial}));return ref;},useEffect:()=>{}};
  if(name==='react/jsx-runtime')return require(name);if(name==='@base-ui/react/dialog')return {Dialog:parts};if(name==='lucide-react')return {X:()=>null};if(name.endsWith("@erpx/ui/lib/cn"))return {cn:(...names:unknown[])=>names.filter(value=>typeof value==='string').join(' ')};if(name.endsWith("@erpx/ui/components/button-variants"))return {buttonVariants:()=>''};if(name.endsWith("@erpx/ui/components/record-context"))return {useEmbeddedRecord:()=>false};if(name.endsWith("@erpx/ui/components/workspace-overlay"))return {useWorkspaceOverlay:()=>({active:false}),useWorkspaceEditor:()=>{}};if(name==='@erpx/ui/components/button')return {Button:primitive('button')};if(name==='@erpx/ui/components/dialog.css')return {};if(name==='@erpx/ui/components/interaction-suspension')return {InteractionSuspension:directoryReact.createContext(false)};throw Error('Unexpected actual Dialog import '+name);
 }});
 const Dialog=loadedModule.exports.Dialog as (props:Record<string,unknown>)=>directoryReact.ReactNode,child=directoryReact.createElement('form',{'data-native-roster-form':'same-original-form'}),render=(open:boolean,retainFormContents=true)=>{cursor=0;const nodes:DirectoryElement[]=[],visit=(value:directoryReact.ReactNode)=>directoryReact.Children.forEach(value,node=>{if(!directoryReact.isValidElement<Record<string,unknown>>(node))return;nodes.push(node);if(typeof node.type==='function')visit((node.type as (props:Record<string,unknown>)=>directoryReact.ReactNode)(node.props));else visit(node.props.children as directoryReact.ReactNode);});visit(Dialog({open,retainFormContents,presentation,title:'Roster planning',onClose:()=>{},children:child}));return nodes;};
 expect(render(false).some(node=>node.props['data-native-roster-form'])).toBe(false);expect(render(true).some(node=>node===child)).toBe(true);const closed=render(false);expect(closed.find(node=>node.type==='dialog-portal')?.props.keepMounted).toBe(true);expect(closed.some(node=>node===child)).toBe(true);expect(closed.find(node=>node.type==='dialog-viewport')?.props.hidden).toBe(true);expect(render(true).some(node=>node===child)).toBe(true);expect(render(false,false).some(node=>node===child)).toBe(false);
});
it('permits correction only after a first exact native ROSTER_CHANGED rollback and never releases that code after an unknown result',async()=>{
 const f=rosterOwnerFixture();try{const input={...rosterTemplateInput,id:'template',expectedVersion:1};f.send.mockResolvedValueOnce(directoryReply({code:'ROSTER_CHANGED',message:'Refresh the actual changed template'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'template',version:3})).mockRejectedValueOnce(Error('Earlier response unknown')).mockResolvedValueOnce(directoryReply({code:'ROSTER_CHANGED',message:'A changed source after uncertainty'},undefined,409));await expect(f.commands.send(input,'2026-10-19')).rejects.toThrow('actual changed template');expect(f.commands.hasPending()).toBe(false);expect(f.commands.isDirty()).toBe(true);f.current={...f.current,data:{...f.current.data,templates:f.current.data.templates.map(template=>({...template,version:2}))}};const corrected={...input,expectedVersion:2,name:'Corrected current template'},result=await f.commands.send(corrected,'2026-10-19');result.acknowledge();expect(f.commands.hasPending()).toBe(false);await expect(f.commands.send(corrected,'2026-10-19')).rejects.toThrow('Earlier response unknown');await expect(f.commands.retry()).rejects.toThrow('after uncertainty');expect(f.commands.hasPending()).toBe(true);expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2','owner-key-3','owner-key-3']);}finally{f.cleanup();}
});

it('keeps the duplicate Monday form editable only after a first native week.create ROSTER_EXISTS rollback, then uses a fresh key for the corrected Monday',async()=>{
 const f=rosterOwnerFixture();try{
  const original={action:'week.create',departmentId:'department',weekStart:'2026-10-19',timezone:'Asia/Bangkok'},corrected={...original,weekStart:'2026-10-26'};
  f.send.mockResolvedValueOnce(directoryReply({code:'ROSTER_EXISTS',message:'This department already has a roster for that week.'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-2',version:1,state:'draft'}));
  f.commands.changed();await expect(f.commands.send(original,'2026-10-19')).rejects.toThrow('already has a roster');expect(f.commands.hasPending()).toBe(false);expect(f.commands.isDirty()).toBe(true);
  f.current={...f.current,data:{...f.current.data,asOf:8}};f.commands.changed();const saved=await f.commands.send(corrected,'2026-10-26');expect(saved.data).toEqual({id:'roster-owner-key-2',version:1,state:'draft'});expect(f.commands.hasPending()).toBe(true);saved.acknowledge();expect(f.commands.hasPending()).toBe(false);expect(f.commands.isDirty()).toBe(false);
  expect(f.send.mock.calls.map(call=>call[1]?.body)).toEqual([JSON.stringify(original),JSON.stringify(corrected)]);expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2']);
 }finally{f.cleanup();}
});
it('never releases week.create ROSTER_EXISTS after a prior unknown outcome, and refuses a newer Monday while preserving the original body/key',async()=>{
 const f=rosterOwnerFixture();try{
  const original={action:'week.create',departmentId:'department',weekStart:'2026-10-19',timezone:'Asia/Bangkok'};f.send.mockRejectedValueOnce(Error('Original week-create result lost')).mockResolvedValueOnce(directoryReply({code:'ROSTER_EXISTS',message:'This department already has a roster for that week.'},undefined,409));
  await expect(f.commands.send(original,'2026-10-19')).rejects.toThrow('result lost');f.commands.changed();await expect(f.commands.retry()).rejects.toThrow('already has a roster');expect(f.commands.hasPending()).toBe(true);expect(f.commands.isDirty()).toBe(true);await expect(f.commands.send({...original,weekStart:'2026-10-26'},'2026-10-26')).rejects.toThrow('Recover the original');expect(()=>f.commands.open()).toThrow('Recover the original');
  expect(f.send).toHaveBeenCalledTimes(2);expect(f.send.mock.calls.map(call=>call[1]?.body)).toEqual([JSON.stringify(original),JSON.stringify(original)]);expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-1']);
 }finally{f.cleanup();}
});
it('lets an empty draft week return to actual shift preparation, then publishes with the fresh native week version and independent command keys',async()=>{
 const f=rosterOwnerFixture();try{
  f.current.data.week!.state='draft';f.current.data.weeks[0].state='draft';f.current.data.shifts=[];
  f.send.mockResolvedValueOnce(directoryReply({code:'ROSTER_EMPTY',message:'Add at least one shift before publishing.'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-2',version:1})).mockResolvedValueOnce(directoryReply({id:'week',version:3,state:'published'}));
  await expect(f.commands.send({action:'week.publish',id:'week',expectedVersion:1},'2026-10-19')).rejects.toThrow('Add at least one shift');expect(f.commands.hasPending()).toBe(false);expect(()=>f.commands.open()).not.toThrow();
  const shift={action:'shift.save',weekId:'week',expectedVersion:1,date:'2026-10-20',name:'Prepared first shift',startTime:'06:00',endTime:'14:00',capacity:2,personIds:[],reason:''},saved=await f.commands.send(shift,'2026-10-19');saved.acknowledge();f.current={...f.current,data:{...f.current.data,asOf:9,week:{...f.current.data.week!,version:2},weeks:f.current.data.weeks.map(week=>({...week,version:2})),shifts:[{...roster().shifts[0],id:'roster-owner-key-2',local_date:'2026-10-20',name:'Prepared first shift'}]}};
  const published=await f.commands.send({action:'week.publish',id:'week',expectedVersion:2},'2026-10-19');published.acknowledge();expect(published.data).toEqual({id:'week',version:3,state:'published'});expect(f.commands.hasPending()).toBe(false);expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2','owner-key-3']);expect(JSON.parse(String(f.send.mock.calls[2][1]?.body))).toMatchObject({action:'week.publish',expectedVersion:2});
 }finally{f.cleanup();}
});
it('retains actual shift fields after a first inactive-template rollback and permits the corrected template with a fresh key',async()=>{
 const f=rosterOwnerFixture();try{const original={action:'shift.save',id:'shift',weekId:'week',expectedVersion:1,templateId:'template',date:'2026-10-19',name:'Prepared shift fields',startTime:'06:00',endTime:'14:00',capacity:2,personIds:[],reason:'Reviewed shift change'};f.send.mockResolvedValueOnce(directoryReply({code:'ROSTER_TEMPLATE',message:'Choose an active template from this department.'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'shift',version:2}));f.commands.changed();await expect(f.commands.send(original,'2026-10-19')).rejects.toThrow('active template');expect(f.commands.hasPending()).toBe(false);expect(f.commands.isDirty()).toBe(true);const corrected={...original,templateId:undefined};f.commands.changed();const result=await f.commands.send(corrected,'2026-10-19');result.acknowledge();expect(result.data).toEqual({id:'shift',version:2});expect(JSON.parse(String(f.send.mock.calls[1][1]?.body))).toMatchObject({name:'Prepared shift fields',startTime:'06:00',endTime:'14:00'});expect(JSON.parse(String(f.send.mock.calls[1][1]?.body))).not.toHaveProperty('templateId');expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2']);}finally{f.cleanup();}
});
it('allows a copied-week local-time rollback to be corrected as a fresh empty week without replaying a born native receipt',async()=>{
 const f=rosterOwnerFixture();try{const original={action:'week.create',departmentId:'department',weekStart:'2026-10-26',timezone:'Asia/Bangkok',copyWeekId:'week'};f.send.mockResolvedValueOnce(directoryReply({code:'ROSTER_TIME',message:'This local time does not exist in the roster timezone.'},undefined,422)).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-2',version:1,state:'draft'}));await expect(f.commands.send(original,'2026-10-26')).rejects.toThrow('local time');expect(f.commands.hasPending()).toBe(false);const result=await f.commands.send({...original,copyWeekId:undefined},'2026-10-26');result.acknowledge();expect(JSON.parse(String(f.send.mock.calls[1][1]?.body))).not.toHaveProperty('copyWeekId');expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2']);}finally{f.cleanup();}
});
it.each(['ROSTER_STARTED','ROSTER_COVERAGE'] as const)('lets a known %s cancellation rollback return to another current shift without treating the rejected cancellation as saved',async code=>{
 const f=rosterOwnerFixture();try{f.send.mockResolvedValueOnce(directoryReply({code,message:'Current native cancellation cannot proceed.'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'other-shift',version:2,state:'cancelled'}));await expect(f.commands.send({action:'shift.cancel',id:'shift',expectedVersion:1,reason:'Reviewed cancellation'},'2026-10-19')).rejects.toThrow('cannot proceed');expect(f.commands.hasPending()).toBe(false);expect(()=>f.commands.open()).not.toThrow();f.current={...f.current,data:{...f.current.data,asOf:10,shifts:[...f.current.data.shifts,{...f.current.data.shifts[0],id:'other-shift'}]}};const result=await f.commands.send({action:'shift.cancel',id:'other-shift',expectedVersion:1,reason:'Cancel the other future shift'},'2026-10-19');result.acknowledge();expect(result.data).toEqual({id:'other-shift',version:2,state:'cancelled'});expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2']);}finally{f.cleanup();}
});
it.each(['ROSTER_DUPLICATE','ROSTER_FULL','ROSTER_CONFLICT','ROSTER_STAFF','ROSTER_SCOPE'] as const)('permits a first native %s request rejection to be corrected to another available target while retaining the request reason',async code=>{
 const f=rosterOwnerFixture();try{const original={action:'request.create',kind:'extra',targetShiftId:'shift',reason:'My retained shift request reason'};f.send.mockResolvedValueOnce(directoryReply({code,message:'Choose a different current target.'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'roster-owner-key-2',version:1,state:'pending_manager'}));f.commands.changed();await expect(f.commands.send(original,'2026-10-19')).rejects.toThrow('different current target');expect(f.commands.hasPending()).toBe(false);expect(f.commands.isDirty()).toBe(true);f.current={...f.current,data:{...f.current.data,asOf:11,shifts:[...f.current.data.shifts,{...f.current.data.shifts[0],id:'another-current-shift'}]}};const result=await f.commands.send({...original,targetShiftId:'another-current-shift'},'2026-10-19');result.acknowledge();expect(result.data).toEqual({id:'roster-owner-key-2',version:1,state:'pending_manager'});expect(JSON.parse(String(f.send.mock.calls[1][1]?.body))).toMatchObject({targetShiftId:'another-current-shift',reason:original.reason});expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2']);}finally{f.cleanup();}
});
it.each(['ROSTER_COWORKER_REQUIRED','ROSTER_COVERAGE'] as const)('does not strand an independently reviewable request after a known %s approval rollback and allows its native rejection',async code=>{
 const f=rosterOwnerFixture();try{f.current.data.requests=[{id:'request',company_id:'company',department_id:'department',kind:'extra',requester_person_id:'person',coworker_person_id:null,requester_name:'Requester',coworker_name:null,source_assignment_id:null,source_version:null,target_shift_id:'shift',target_assignment_id:null,target_version:null,state:'pending_manager',reason:'Original request',decision_note:'',version:3,created_at:1,permittedActions:['approve','reject']}];f.send.mockResolvedValueOnce(directoryReply({code,message:'This approval cannot proceed yet.'},undefined,409)).mockResolvedValueOnce(directoryReply({id:'request',version:4,state:'rejected'}));await expect(f.commands.send({action:'request.decide',id:'request',expectedVersion:3,decision:'approve',reason:'Independent approval'},'2026-10-19')).rejects.toThrow('cannot proceed');expect(f.commands.hasPending()).toBe(false);const result=await f.commands.send({action:'request.decide',id:'request',expectedVersion:3,decision:'reject',reason:'Independent rejection after review'},'2026-10-19');result.acknowledge();expect(result.data).toEqual({id:'request',version:4,state:'rejected'});expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-2']);}finally{f.cleanup();}
});
it.each(['ROSTER_TEMPLATE','ROSTER_EMPTY','ROSTER_STARTED','ROSTER_DUPLICATE','ROSTER_COWORKER_REQUIRED'] as const)('keeps the first original body/key when a later exact native %s follows an earlier unknown response',async code=>{
 const f=rosterOwnerFixture();try{
  f.current.data.requests=[{id:'request',company_id:'company',department_id:'department',kind:'extra',requester_person_id:'person',coworker_person_id:null,requester_name:'Requester',coworker_name:null,source_assignment_id:null,source_version:null,target_shift_id:'shift',target_assignment_id:null,target_version:null,state:'pending_manager',reason:'Original request',decision_note:'',version:3,created_at:1,permittedActions:['approve','reject']}];
  const input=code==='ROSTER_TEMPLATE'?{action:'shift.save',id:'shift',weekId:'week',expectedVersion:1,templateId:'template',date:'2026-10-19',name:'Original shift',startTime:'06:00',endTime:'14:00',capacity:2,personIds:[],reason:'Reviewed change'}:code==='ROSTER_EMPTY'?{action:'week.publish',id:'week',expectedVersion:1}:code==='ROSTER_STARTED'?{action:'week.create',departmentId:'department',weekStart:'2026-10-26',timezone:'Asia/Bangkok',copyWeekId:'week'}:code==='ROSTER_DUPLICATE'?{action:'request.create',kind:'extra',targetShiftId:'shift',reason:'Original request reason'}:{action:'request.decide',id:'request',expectedVersion:3,decision:'approve',reason:'Original independent approval'};
  f.send.mockRejectedValueOnce(Error('Earlier response unknown')).mockResolvedValueOnce(directoryReply({code,message:'Original native rollback code after uncertainty.'},undefined,409));await expect(f.commands.send(input,'2026-10-19')).rejects.toThrow('Earlier response unknown');f.commands.changed();await expect(f.commands.retry()).rejects.toThrow('after uncertainty');expect(f.commands.hasPending()).toBe(true);expect(f.commands.isDirty()).toBe(true);expect(()=>f.commands.open()).toThrow('Recover the original');expect(f.send.mock.calls.map(call=>call[1]?.body)).toEqual([JSON.stringify(input),JSON.stringify(input)]);expect(f.send.mock.calls.map(call=>new Headers(call[1]?.headers).get('Idempotency-Key'))).toEqual(['owner-key-1','owner-key-1']);
 }finally{f.cleanup();}
});
it.each([
 {code:'ROSTER_STARTED',status:409,input:{action:'week.create',departmentId:'department',weekStart:'2026-10-26',timezone:'Asia/Bangkok'}},
 {code:'ROSTER_TIME',status:422,input:{action:'week.create',departmentId:'department',weekStart:'2026-10-26',timezone:'Asia/Bangkok'}},
 {code:'ROSTER_TEMPLATE',status:422,input:{action:'shift.save',id:'shift',weekId:'week',expectedVersion:1,date:'2026-10-19',name:'Shift',startTime:'06:00',endTime:'14:00',capacity:2,personIds:[],reason:'Reviewed change'}},
 {code:'ROSTER_STAFF',status:403,input:{action:'request.create',kind:'extra',targetShiftId:'shift',reason:'Original request reason'}},
 {code:'ROSTER_SCOPE',status:403,input:{action:'week.create',departmentId:'department',weekStart:'2026-10-26',timezone:'Asia/Bangkok',copyWeekId:'week'}},
] as const)('does not infer rollback for $code outside its exact native action/status path',async({code,status,input})=>{
 const f=rosterOwnerFixture();try{f.send.mockResolvedValueOnce(directoryReply({code,message:'Do not release this unmatched path.'},undefined,status));await expect(f.commands.send(input,'2026-10-19')).rejects.toThrow('unmatched path');expect(f.commands.hasPending()).toBe(true);expect(f.commands.isDirty()).toBe(true);}finally{f.cleanup();}
});

it('mounted standard Calendar uses its SSR snapshot, opens the actual appointment drawer and preserves the editor inputs without a mount read',()=>{
 const browser=rosterOwnerBrowser(),send=vi.fn<typeof directoryFetch>(),dummy=mountRosterOwner('calendar',{},send),component=mountRosterOwner('calendar',{base:'/holdings/acme',initial:{...dummy.calendar,canSchedule:true},initialHierarchy:dummy.hierarchy,initialAccounts:[],contextKey:'actual-calendar-company'},send);
 try{
  let nodes=component.render();expect(nodes.some(node=>node.props['data-calendar-workspace']!==undefined&&node.props['data-ui-appearance']==='standard')).toBe(true);expect(send).not.toHaveBeenCalled();
  (rosterNodeText(nodes,'New appointment').props.onClick as ()=>void)();nodes=component.render();const drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='New appointment')!;expect(drawer.props.presentation).toBe('form-drawer');expect(drawer.props.appearance).toBe('standard');
  const editor=component.within(drawer).find(node=>node.props.editor)!;expect(editor.props.editor).toEqual({kind:'appointment'});expect(editor.props.hierarchy).toBe(dummy.hierarchy);expect(editor.props.date).toBe('2026-10-19');expect(editor.props.api).toBe('/api/v1/holdings/acme/calendar');expect(send).not.toHaveBeenCalled();
 }finally{component.close();dummy.close();browser.cleanup();}
});
it('mounted standard Calendar view navigation preserves the native date, timezone and filters in its exact owner GET and history URL',async()=>{
 const browser=rosterOwnerBrowser(),send=vi.fn<typeof directoryFetch>(),dummy=mountRosterOwner('calendar',{},send),query={...dummy.calendar.query,date:'2026-10-19',timezone:'Asia/Bangkok',person:'person-current',search:'Native search',layers:'roster,leave'},data={...dummy.calendar,query},component=mountRosterOwner('calendar',{base:'/holdings/acme',initial:data,initialHierarchy:dummy.hierarchy,initialAccounts:[],contextKey:'actual-calendar-company'},send);
 send.mockResolvedValue(directoryReply({...data,query:{...query,view:'agenda',cursor:0}}));
 try{
  const nodes=component.render();expect(send).not.toHaveBeenCalled();(rosterNodeText(nodes,'Agenda').props.onClick as ()=>void)();await directoryTick();component.render();expect(send).toHaveBeenCalledOnce();const url=new URL(String(send.mock.calls[0][0]),window.location.origin);expect(url.pathname).toBe('/api/v1/holdings/acme/calendar');expect(Object.fromEntries(url.searchParams)).toMatchObject({date:'2026-10-19',timezone:'Asia/Bangkok',person:'person-current',search:'Native search',layers:'roster,leave',view:'agenda',cursor:'0'});
  const history=vi.mocked(window.history.pushState);expect(history).toHaveBeenCalledOnce();const destination=new URL(String(history.mock.calls[0][2]),window.location.origin);expect(Object.fromEntries(destination.searchParams)).toMatchObject({date:'2026-10-19',timezone:'Asia/Bangkok',person:'person-current',search:'Native search',layers:'roster,leave',view:'agenda'});
 }finally{component.close();dummy.close();browser.cleanup();}
});

function calendarRejectedResponse(){let resolve!:(value:Response)=>void,reject!:(reason:unknown)=>void;const promise=new Promise<Response>((done,failed)=>{resolve=done;reject=failed;});return{promise,resolve,reject};}
it('mounted Calendar zero-field task action holds departure during transport and keeps exact retry inside its original detail modal',async()=>{
 const browser=rosterOwnerBrowser(),response=calendarRejectedResponse(),send=vi.fn<typeof directoryFetch>(),dummy=mountRosterOwner('calendar',{},send),item={id:'native-task',source:'lifecycle',sourceId:'native-task',kind:'task',title:'Original no-field task',allDay:true,startDate:'2026-10-19',endDate:'2026-10-20',startsAt:null,endsAt:null,timezone:'Asia/Bangkok',departmentId:null,branchId:null,participants:[],owner:null,status:'pending',version:1,permittedActions:['task.complete'],warnings:[],history:[]},data={...dummy.calendar,items:[item]},component=mountRosterOwner('calendar',{base:'/holdings/acme',initial:data,initialHierarchy:dummy.hierarchy,initialAccounts:[],initialEvent:item.id,contextKey:'original-calendar-owner'},send);
 let originalKey='';send.mockImplementation(async(_url,init)=>{if(init?.method!=='POST')return directoryReply({...data,items:[{...item,status:'completed',version:2}]});originalKey=new Headers(init.headers).get('Idempotency-Key')!;return response.promise;});
 try{
  let nodes=component.render();(rosterNodeText(nodes,'Complete task').props.onClick as ()=>void)();nodes=component.render();let lifetime=nodes.find(node=>node.props.busy===true&&node.props.dirty===false&&'pending'in node.props)!;expect(lifetime).toBeDefined();let detail=nodes.find(node=>node.type==='aside'&&node.props.title===item.title)!;(detail.props.onClose as ()=>void)();expect(component.render().some(node=>node.type==='aside'&&node.props.title===item.title)).toBe(true);
  response.reject(Error('Original no-field response lost'));await directoryTick();nodes=component.render();lifetime=nodes.find(node=>node.props.busy===false&&node.props.pending===true)!;expect(lifetime).toBeDefined();detail=nodes.find(node=>node.type==='aside'&&node.props.title===item.title)!;expect(rosterNodeText(component.within(detail),'Retry submitted change')).toBeDefined();
  send.mockImplementation(async(_url,init)=>init?.method==='POST'?directoryReply({id:item.sourceId,version:2,status:'completed',warnings:[]}):directoryReply({...data,items:[{...item,status:'completed',version:2}]}));(rosterNodeText(component.within(detail),'Retry submitted change').props.onClick as ()=>void)();await directoryTick();component.render();const posts=send.mock.calls.filter(call=>call[1]?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(originalKey);expect(JSON.parse(String(posts[1][1]?.body))).toMatchObject({action:'task.complete',id:item.sourceId,expectedVersion:1});
 }finally{component.close();dummy.close();browser.cleanup();}
});
it('mounted TeamSchedule appointment retains original completion/key and actual drawer through unknown, permission loss and a newer edit',async()=>{
 const browser=rosterOwnerBrowser(),send=vi.fn<typeof directoryFetch>(),component=mountRosterOwner('team',{base:'/holdings/acme',initialWeek:'2026-10-19',initialView:'team',onBack:vi.fn()},send);component.calendar.canSchedule=true;
 let version='original-form',key='';const form={isConnected:true,getAttribute:()=>version},completion={form:form as unknown as HTMLFormElement,formEditVersion:version},input={action:'appointment.save',title:'Original team appointment'};
 send.mockImplementation(async(_url,init)=>{key=new Headers(init?.headers).get('Idempotency-Key')!;if(send.mock.calls.length===1)throw Error('Original team result lost');return directoryReply({id:`calendar-${key}`,version:1,status:'scheduled',warnings:[]});});
 try{
  let nodes=component.render();(rosterNodeText(nodes,'Create team appointment').props.onClick as ()=>void)();nodes=component.render();let drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!,editor=component.within(drawer).find(node=>node.props.editor)!;expect(drawer.props.presentation).toBe('form-drawer');(editor.props.send as (input:unknown,submitted:typeof completion)=>void)(input,completion);await directoryTick();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!;expect(drawer.props.dismissDisabled).toBe(true);expect(drawer.props.retainFormContents).toBe(true);(drawer.props.onClose as ()=>void)();expect(component.render().find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!.props.open).toBe(true);
  component.calendar.canSchedule=false;version='newer-form';editor=component.within(drawer).find(node=>node.props.editor)!;(editor.props.edited as ()=>void)();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!;expect(component.within(drawer).find(node=>node.props.editor)!.props.hierarchy).toBe(editor.props.hierarchy);
  (rosterNodeText(component.within(drawer),'Retry submitted team appointment').props.onClick as ()=>void)();await directoryTick();nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!;expect(drawer.props.open).toBe(true);expect(nodes.some(node=>node.props.children==='Submitted team appointment saved. Your newer draft is retained.')).toBe(true);expect(version).toBe('newer-form');const posts=send.mock.calls.filter(call=>call[1]?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(posts[1][0]).toBe('/api/v1/holdings/acme/calendar/appointments');expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(key);expect(posts[1][2]).toEqual(completion);expect(posts.every(call=>call[3]==='caller')).toBe(true);
 }finally{component.close();browser.cleanup();}
});

function mountedCalendarMove(send:typeof directoryFetch){
 const dummy=mountRosterOwner('calendar',{},send),item={id:'native-meeting',source:'appointments',sourceId:'native-meeting',kind:'appointment',title:'Original timed meeting',allDay:false,startDate:null,endDate:null,startsAt:Date.parse('2026-10-19T02:00:00Z'),endsAt:Date.parse('2026-10-19T03:00:00Z'),timezone:'Asia/Bangkok',departmentId:null,branchId:null,participants:[],owner:null,status:'scheduled',version:1,permittedActions:['appointment.save'],warnings:[],history:[]},data={...dummy.calendar,query:{...dummy.calendar.query,view:'day',mode:'team',preset:'team'},items:[item]},component=mountRosterOwner('calendar',{base:'/holdings/acme',initial:data,initialHierarchy:dummy.hierarchy,initialAccounts:[],contextKey:'original-calendar-owner'},send);
 const timeline=()=>component.render().find(node=>typeof node.props.onReschedule==='function')!;
 return {dummy,item,data,component,timeline,move:()=>{(timeline().props.onReschedule as (item:unknown,change:unknown)=>void)(item,{startsAt:item.startsAt+3600000,endsAt:item.endsAt+3600000});},version:()=>((timeline().props.items as {version:number}[])[0]?.version),close:()=>{component.close();dummy.close();}};
}
it('mounted Calendar late failed move cannot restore its old record over a newer native owner read',async()=>{
 const browser=rosterOwnerBrowser(),post=calendarRejectedResponse(),send=vi.fn<typeof directoryFetch>(),f=mountedCalendarMove(send);send.mockImplementation(async(_url,init)=>init?.method==='POST'?post.promise:directoryReply({...f.data,items:[{...f.item,version:9,title:'Later independently read meeting'}]}));
 try{
  f.move();f.component.render();Object.assign(document,{visibilityState:'visible'});window.dispatchEvent(new Event('focus'));await directoryTick();expect(f.version()).toBe(9);
  post.reject(Error('Old move failed after newer read'));await directoryTick();const nodes=f.component.render();expect(f.version()).toBe(9);expect(nodes.some(node=>node.props.children==='Old move failed after newer read')).toBe(false);expect(rosterNodeText(nodes,'Retry submitted change')).toBeDefined();expect(send.mock.calls.filter(call=>call[1]?.method==='POST')).toHaveLength(1);
 }finally{f.close();browser.cleanup();}
});
it('mounted Calendar suppresses failed move state after Work changes and restores exact recovery on return to its original owner',async()=>{
 const browser=rosterOwnerBrowser(),post=calendarRejectedResponse(),send=vi.fn<typeof directoryFetch>(),f=mountedCalendarMove(send);send.mockImplementation(async(_url,init)=>init?.method==='POST'?post.promise:directoryReply({...f.data,items:[{...f.item,version:2}]}));const revision=browser.node.dataset.workRevision;
 try{
  f.move();f.component.render();browser.node.dataset.workRevision='changed-role';post.reject(Error('Old role failure must not publish'));await directoryTick();let nodes=f.component.render();expect(nodes.some(node=>node.props.children==='Old role failure must not publish')).toBe(false);
  browser.node.dataset.workRevision=revision;window.dispatchEvent(new Event('erpx:work-context-changed'));nodes=f.component.render();const retry=rosterNodeText(nodes,'Retry submitted change');expect(retry.props.disabled).toBe(false);send.mockImplementation(async(_url,init)=>init?.method==='POST'?directoryReply({id:f.item.id,version:2,status:'scheduled',warnings:[]}):directoryReply({...f.data,items:[{...f.item,version:2}]}));(retry.props.onClick as ()=>void)();await directoryTick();f.component.render();const posts=send.mock.calls.filter(call=>call[1]?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));
 }finally{f.close();browser.cleanup();}
});
it('mounted Calendar does not ACK or announce a saved move when ORIGINAL refresh JSON belongs to a lost Work owner',async()=>{
 const browser=rosterOwnerBrowser(),started=directoryDeferred<void>(),release=directoryDeferred<void>(),refreshed=directoryDeferred<void>(),send=vi.fn<typeof directoryFetch>(),f=mountedCalendarMove(send);let reads=0;send.mockImplementation(async(_url,init)=>{
  if(init?.method==='POST')return directoryReply({id:f.item.id,version:2,status:'scheduled',warnings:[]});
  const response=directoryReply({...f.data,items:[{...f.item,version:9}]},async()=>{if(++reads===1){started.resolve();await release.promise;}}),decode=response.json.bind(response);
  response.json=async()=>{const value=await decode();if(reads===2)refreshed.resolve();return value;};return response;
 });const revision=browser.node.dataset.workRevision;
 try{
  f.move();await started.promise;browser.node.dataset.workRevision='other-current-role';release.resolve();await directoryTick();let nodes=f.component.render();expect(f.version()).toBe(1);expect(nodes.some(node=>node.props.children==='Calendar updated.')).toBe(false);expect(nodes.some(node=>typeof node.props.children==='string'&&node.props.children.includes('role changed'))).toBe(false);
  browser.node.dataset.workRevision=revision;window.dispatchEvent(new Event('erpx:work-context-changed'));nodes=f.component.render();const retry=rosterNodeText(nodes,'Retry submitted change');expect(retry.props.disabled).toBe(false);(retry.props.onClick as ()=>void)();await refreshed.promise;await directoryTick();nodes=f.component.render();expect(f.version()).toBe(9);expect(nodes.some(node=>node.props.children==='Calendar updated.')).toBe(true);const posts=send.mock.calls.filter(call=>call[1]?.method==='POST');expect(posts).toHaveLength(2);expect(posts[1][1]?.body).toBe(posts[0][1]?.body);expect(new Headers(posts[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(posts[0][1]?.headers).get('Idempotency-Key'));
 }finally{f.close();browser.cleanup();}
});
it('mounted TeamSchedule suppresses post-refresh source loss and keeps the original appointment completion for recovery on return',async()=>{
 const browser=rosterOwnerBrowser(),started=directoryDeferred<void>(),release=directoryDeferred<void>(),send=vi.fn<typeof directoryFetch>();let reloads=0;const reload=vi.fn(async(endpoint:string)=>{if(endpoint.includes('/calendar?')&&++reloads===1){started.resolve();await release.promise;}}),component=mountRosterOwner('team',{base:'/holdings/acme',initialWeek:'2026-10-19',initialView:'team',onBack:vi.fn()},send,reload);component.calendar.canSchedule=true;const revision=browser.node.dataset.workRevision,form={isConnected:true,getAttribute:()=> 'original-form'},completion={form:form as unknown as HTMLFormElement,formEditVersion:'original-form'},input={action:'appointment.save',title:'Original retained team appointment'};
 send.mockImplementation(async(_url,init)=>directoryReply({id:`calendar-${new Headers(init?.headers).get('Idempotency-Key')}`,version:1,status:'scheduled',warnings:[]}));
 try{
  (rosterNodeText(component.render(),'Create team appointment').props.onClick as ()=>void)();let drawer=component.render().find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!;const editor=component.within(drawer).find(node=>node.props.editor)!;(editor.props.send as (input:unknown,submitted:typeof completion)=>void)(input,completion);await started.promise;browser.node.dataset.workRevision='changed-role';release.resolve();await directoryTick();let nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!;expect(drawer.props.open).toBe(true);expect(nodes.some(node=>node.props.children==='Team appointment saved to Calendar.')).toBe(false);
  browser.node.dataset.workRevision=revision;window.dispatchEvent(new Event('erpx:work-context-changed'));nodes=component.render();drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!;const retry=rosterNodeText(component.within(drawer),'Retry submitted team appointment');expect(retry.props.disabled).toBe(false);(retry.props.onClick as ()=>void)();await directoryTick();nodes=component.render();expect(nodes.some(node=>node.type==='aside'&&node.props.title==='Create team appointment'&&node.props.open)).toBe(false);expect(nodes.some(node=>node.props.children==='Team appointment saved to Calendar.')).toBe(true);expect(send).toHaveBeenCalledTimes(2);expect(send.mock.calls[1][1]?.body).toBe(send.mock.calls[0][1]?.body);expect(send.mock.calls[1][2]).toEqual(completion);expect(new Headers(send.mock.calls[1][1]?.headers).get('Idempotency-Key')).toBe(new Headers(send.mock.calls[0][1]?.headers).get('Idempotency-Key'));
 }finally{component.close();browser.cleanup();}
});

it('the actual specialized Team appointment selects legacy fields while retaining its native Calendar owner and completion port',()=>{
 const browser=rosterOwnerBrowser(),send=vi.fn<typeof directoryFetch>(),component=mountRosterOwner('team',{base:'/holdings/acme',initialWeek:'2026-10-19',initialView:'team',onBack:vi.fn()},send);component.calendar.canSchedule=true;
 try{let nodes=component.render();(rosterNodeText(nodes,'Create team appointment').props.onClick as()=>void)();nodes=component.render();const drawer=nodes.find(node=>node.type==='aside'&&node.props.title==='Create team appointment')!,editor=component.within(drawer).find(node=>node.props.editor)!;expect(editor.props.appearance).toBe('legacy');expect(editor.props.editor).toEqual({kind:'appointment'});expect(editor.props.api).toBe('/api/v1/holdings/acme/calendar');expect(editor.props.hierarchy).toBe(component.hierarchy);expect(typeof editor.props.send).toBe('function');expect(typeof editor.props.edited).toBe('function');expect(typeof editor.props.close).toBe('function');expect(drawer.props.presentation).toBe('form-drawer');expect(send).not.toHaveBeenCalled();}finally{component.close();browser.cleanup();}
});
