import { createFinanceCommandFetch } from "@erpx/feature-finance/public/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import {
  announceContextSwitch,
  confirmContextSwitch,
  contextGeneration,
  contextIsSwitching,
  registerContextSwitchBlocker,
  hasUnsavedWork,
  registerUnsavedWork,
  setContextSwitching,
} from "@erpx/feature-work_projection/lib/context-switch";

type ContextNode = { isConnected: boolean; dataset: Record<string, string> };
let context: ContextNode;
let platform: ContextNode;
let view: EventTarget & { location: { origin: string }; confirm: ReturnType<typeof vi.fn> };
let dirtyMarker: boolean;
let documentElement: { dataset: Record<string, string> };
let channels: { name: string; postMessage: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn> }[];
const subscriptions: (() => void)[] = [];

beforeEach(() => {
  context = { isConnected: true, dataset: { workUser: "user-a", workCompany: "company-a", workAssignment: "assignment-a", workRevision: "7", workAssignmentVersion: "3", workStale: "false" } };
  platform = { isConnected: true, dataset: { platformAssignment: "platform-a" } };
  dirtyMarker = false;
  documentElement = { dataset: {} };
  channels = [];
  view = Object.assign(new EventTarget(), { location: { origin: "https://erpx.test" }, confirm: vi.fn(() => true) });
  vi.stubGlobal("window", view);
  vi.stubGlobal("document", {
    documentElement,
    querySelector(selector: string) {
      if (selector === "[data-work-revision]") return context;
      if (selector === "[data-platform-assignment]") return platform;
      if (selector === '[data-work-dirty="true"]') return dirtyMarker ? context : null;
      return null;
    },
    querySelectorAll: () => [context],
  });
  vi.stubGlobal("BroadcastChannel", class {
    postMessage = vi.fn();
    close = vi.fn();
    constructor(public name: string) { channels.push(this); }
  });
  setContextSwitching(false);
});

afterEach(() => {
  subscriptions.splice(0).forEach(unregister => unregister());
  setContextSwitching(false);
  vi.unstubAllGlobals();
});

async function expectContextConflict(response: Response) {
  expect(response.status).toBe(409);
  expect(await response.json()).toMatchObject({ error: { code: "WORK_CONTEXT_CHANGED" } });
}

it.each(["POST", "PUT", "PATCH", "DELETE"])("blocks a pending %s command before it can reach the server", async method => {
  const fetcher = vi.fn();
  vi.stubGlobal("fetch", fetcher);
  setContextSwitching(true);
  await expectContextConflict(await workFetch("/api/v1/holdings/acme/people/one", { method }));
  expect(fetcher).not.toHaveBeenCalled();
});

it("lets the intentional Platform selector run while retaining rendered Holding and Platform authority", async () => {
  const fetcher = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () => Response.json({ data: { selected: true } }));
  vi.stubGlobal("fetch", fetcher);
  setContextSwitching(true);
  const response = await workFetch("/api/v1/platform/assignments/active", {
    method: "POST", headers: { "content-type": "application/json", "x-erpx-context-company": "attempted-company" },
    body: JSON.stringify({ assignmentId: "platform-b" }),
  });
  expect(response.status).toBe(200);
  expect(fetcher).toHaveBeenCalledTimes(1);
  const init = fetcher.mock.calls[0][1] as RequestInit;
  const headers = new Headers(init.headers);
  expect(headers.get("content-type")).toBe("application/json");
  expect(headers.get("x-erpx-context-company")).toBe("company-a");
  expect(headers.get("x-erpx-context-assignment")).toBe("assignment-a");
  expect(headers.get("x-erpx-context-revision")).toBe("7");
  expect(headers.get("x-erpx-context-assignment-version")).toBe("3");
  expect(headers.get("x-erpx-platform-assignment")).toBe("platform-a");
});

it.each(["/api/v1/holdings/acme/push", "/api/v1/holdings/acme/push/session", "/api/v1/front/acme/company/pms/push"])("allows Push cleanup during a pending switch at %s", async path => {
  const fetcher = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () => new Response(null, { status: 204 }));
  vi.stubGlobal("fetch", fetcher);
  setContextSwitching(true);
  expect((await workFetch(path, { method: "DELETE" })).status).toBe(204);
  expect(fetcher).toHaveBeenCalledTimes(1);
  const headers = new Headers((fetcher.mock.calls[0][1] as RequestInit).headers);
  expect(headers.get("x-erpx-context-company")).toBe("company-a");
  expect(headers.get("x-erpx-context-assignment")).toBe("assignment-a");
  expect(headers.get("x-erpx-context-revision")).toBe("7");
  expect(headers.get("x-erpx-context-assignment-version")).toBe("3");
});

it("keeps an intentional Holding selector's caller headers and revision during a switch", async () => {
  const fetcher = vi.fn<(input: RequestInfo | URL, init?: RequestInit) => Promise<Response>>(async () => Response.json({ data: {} }));
  vi.stubGlobal("fetch", fetcher);
  setContextSwitching(true);
  const init = { method: "POST", headers: { "x-erpx-context-company": "company-a" }, body: JSON.stringify({ assignmentId: "assignment-b", revision: 7 }) };
  expect((await workFetch("/api/v1/holdings/acme/assignments/active", init)).status).toBe(200);
  expect(fetcher).toHaveBeenCalledWith("/api/v1/holdings/acme/assignments/active", init);
});

it("replaces an old successful response when a context switch starts before it finishes", async () => {
  let finish!: (value: Response) => void;
  vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { finish = resolve; })));
  const pending = workFetch("/api/v1/holdings/acme/people");
  const before = contextGeneration();
  setContextSwitching(true);
  expect(contextGeneration()).toBeGreaterThan(before);
  finish(Response.json({ data: { privateDetail: "old account data" } }));
  const response = await pending;
  expect(response.status).toBe(409);
  const body = await response.json();
  expect(body).toMatchObject({ error: { code: "WORK_CONTEXT_CHANGED" } });
  expect(JSON.stringify(body)).not.toContain("old account data");
});

it.each(["removed", "user", "company", "assignment", "assignmentVersion", "revision", "stale"])("rejects a late response when rendered context is %s", async change => {
  let finish!: (value: Response) => void;
  vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { finish = resolve; })));
  const pending = workFetch("/api/v1/holdings/acme/people");
  if (change === "removed") context.isConnected = false;
  else if (change === "stale") context.dataset.workStale = "true";
  else {
    const fields: Record<string, string> = { user: "workUser", company: "workCompany", assignment: "workAssignment", revision: "workRevision", assignmentVersion: "workAssignmentVersion" };
    context.dataset[fields[change]] = "changed";
  }
  finish(Response.json({ data: { privateDetail: "old account data" } }));
  const response = await pending;
  expect(response.status).toBe(409);
  const body = await response.json();
  expect(body).toMatchObject({ error: { code: "WORK_CONTEXT_CHANGED" } });
  expect(JSON.stringify(body)).not.toContain("old account data");
});

it.each(["removed", "changed"])("rejects a Platform response when its rendered assignment is %s", async change => {
  let finish!: (value: Response) => void;
  vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { finish = resolve; })));
  const pending = workFetch("/api/v1/platform/holdings");
  if (change === "removed") platform.isConnected = false;
  else platform.dataset.platformAssignment = "platform-b";
  finish(Response.json({ data: { holdings: ["old platform data"] } }));
  await expectContextConflict(await pending);
});

it("announces a switch to other tabs and invalidates mounted context before old requests resolve", () => {
  const changed = vi.fn();
  view.addEventListener("erpx:work-context-changed", changed);
  const before = contextGeneration();
  announceContextSwitch();
  expect(contextGeneration()).toBeGreaterThan(before);
  expect(context.dataset.workStale).toBe("true");
  expect(changed).toHaveBeenCalledTimes(1);
  expect(channels[0].name).toBe("erpx-work-context");
  expect(channels[0].postMessage).toHaveBeenCalledWith({ changed: true });
  expect(channels[0].close).toHaveBeenCalledTimes(1);
});

it("asks before discarding registered unsaved work and stops asking once it is unregistered", () => {
  const stop = registerUnsavedWork(() => true);
  subscriptions.push(stop);
  view.confirm.mockReturnValue(false);
  expect(confirmContextSwitch()).toBe(false);
  expect(view.confirm).toHaveBeenCalledTimes(1);
  stop();
  expect(confirmContextSwitch()).toBe(true);
  expect(view.confirm).toHaveBeenCalledTimes(1);
});
it("refuses sign-out when a command is unresolved and uses a sign-out prompt for drafts", () => {
  const stop = registerContextSwitchBlocker(() => "Retry the original Finance action before leaving.");
  subscriptions.push(stop);
  expect(confirmContextSwitch("Sign out and discard unsaved changes?")).toBe(false);
  expect(view.confirm).not.toHaveBeenCalled();
  stop();
  dirtyMarker = true;
  view.confirm.mockReturnValue(false);
  expect(confirmContextSwitch("Sign out and discard unsaved changes?")).toBe(false);
  expect(view.confirm).toHaveBeenCalledWith("Sign out and discard unsaved changes?");
});
it("reads registered and DOM-marked drafts for an app-owned confirmation without opening a native prompt", () => {
  expect(hasUnsavedWork()).toBe(false);
  const stop = registerUnsavedWork(() => true); subscriptions.push(stop);
  expect(hasUnsavedWork()).toBe(true);
  stop(); dirtyMarker = true;
  expect(hasUnsavedWork()).toBe(true);
  dirtyMarker = false;
  expect(hasUnsavedWork()).toBe(false);
  expect(view.confirm).not.toHaveBeenCalled();
});
it('identifies the form that actually saved without marking another draft or a failed command saved',async()=>{
 const editor={id:'editor'},attachment={id:'attachment'};let active=editor;
 Object.defineProperty(document,'activeElement',{configurable:true,value:{closest:()=>active}});
 const saved:unknown[]=[];view.addEventListener('erpx:record-command-saved',event=>saved.push((event as CustomEvent).detail.form));
 let finish!:(response:Response)=>void;
 vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve;})));
 const pending=workFetch('/api/v1/holdings/acme/commerce/products/one',{method:'POST'});
 active=attachment;finish(Response.json({data:{id:'one'}}));await pending;
 expect(saved).toEqual([editor]);
 vi.stubGlobal('fetch',vi.fn(async()=>Response.json({error:{code:'CONFLICT'}},{status:409})));
 await workFetch('/api/v1/holdings/acme/commerce/products/one',{method:'POST'});
 expect(saved).toEqual([editor]);
});
it('acknowledges the submitted field version instead of later edits made while the server is saving',async()=>{
 let fields='submitted-fields',finish!:(response:Response)=>void;
 const form={getAttribute:(name:string)=>name==='data-erpx-edit-version'?fields:null};
 Object.defineProperty(document,'activeElement',{configurable:true,value:{closest:()=>form}});
 const saved:{form:unknown;formEditVersion:string|null}[]=[];
 view.addEventListener('erpx:record-command-saved',event=>saved.push((event as CustomEvent).detail));
 vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve;})));
 const pending=workFetch('/api/v1/holdings/acme/finance/charts',{method:'POST'});
 fields='later-unsaved-fields';finish(Response.json({data:{id:'saved-chart'}}));await pending;
 expect(saved).toEqual([{form,formEditVersion:'submitted-fields'}]);
 expect(saved[0].formEditVersion).not.toBe(form.getAttribute('data-erpx-edit-version'));
});

it.each(['none','another form'])('attributes an asynchronous upload completion to its submitted form with %s focused',async focus=>{
 const form={getAttribute:()=> 'submitted-fields'} as unknown as HTMLFormElement;
 const unrelated={getAttribute:()=> 'unrelated-fields'};
 Object.defineProperty(document,'activeElement',{configurable:true,value:focus==='none'?null:{closest:()=>unrelated}});
 const saved:unknown[]=[];view.addEventListener('erpx:record-command-saved',event=>saved.push((event as CustomEvent).detail));
 const network=vi.fn(async(_input:RequestInfo|URL,_init?:RequestInit)=>Response.json({data:{id:'signed-copy'}}));vi.stubGlobal('fetch',network);
 await workFetch('/api/v1/holdings/acme/legal/items/one/executed-copies',{method:'POST',body:'original request body'},{form,formEditVersion:'submitted-fields'});
 expect(saved).toEqual([{form,formEditVersion:'submitted-fields'}]);
 expect(network.mock.calls[0]).toHaveLength(2);
 expect(network.mock.calls[0][1]).toMatchObject({method:'POST',body:'original request body'});
});
it('retains the explicit submitted version across asynchronous preparation and newer edits',async()=>{
 let fields='submitted-fields',finish!:(response:Response)=>void;
 const form={getAttribute:()=>fields} as unknown as HTMLFormElement;
 const completion={form,formEditVersion:form.getAttribute('data-erpx-edit-version')};
 fields='edit-during-file-hash';
 const saved:{form:unknown;formEditVersion:string|null}[]=[];view.addEventListener('erpx:record-command-saved',event=>saved.push((event as CustomEvent).detail));
 vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve;})));
 const pending=workFetch('/api/v1/holdings/acme/legal/items/one/executed-copies',{method:'POST'},completion);
 fields='edit-during-server-save';finish(Response.json({data:{id:'signed-copy'}}));await pending;
 expect(saved).toEqual([{form,formEditVersion:'submitted-fields'}]);
 expect(saved[0].formEditVersion).not.toBe(form.getAttribute('data-erpx-edit-version'));
});
it.each(['server refusal','context changed'])('does not acknowledge an explicit upload form when %s',async failure=>{
 const form={getAttribute:()=> 'submitted-fields'} as unknown as HTMLFormElement;
 const saved=vi.fn();view.addEventListener('erpx:record-command-saved',saved);
 let finish!:(response:Response)=>void;vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve;})));
 const pending=workFetch('/api/v1/holdings/acme/legal/items/one/executed-copies',{method:'POST'},{form,formEditVersion:'submitted-fields'});
 if(failure==='context changed')context.dataset.workRevision='8';
 finish(failure==='server refusal'?Response.json({error:{code:'COPY_CONFLICT'}},{status:409}):Response.json({data:{id:'signed-copy'}}));
 expect((await pending).status).toBe(409);expect(saved).not.toHaveBeenCalled();
});

it("also confirms a dirty rendered form and exposes pending state until switching finishes", () => {
  dirtyMarker = true;
  view.confirm.mockReturnValue(false);
  expect(confirmContextSwitch()).toBe(false);
  setContextSwitching(true);
  expect(contextIsSwitching()).toBe(true);
  expect(documentElement.dataset.workSwitching).toBe("true");
  setContextSwitching(false);
  expect(contextIsSwitching()).toBe(false);
  expect(documentElement.dataset.workSwitching).toBe("false");
});

it("retains a Finance draft through a truncated success and acknowledges only the original version after parsed retry",async()=>{
 let fields="submitted-finance-fields",finish!:(response:Response)=>void;
 const form={getAttribute:()=>fields} as unknown as HTMLFormElement;
 Object.defineProperty(document,"activeElement",{configurable:true,value:{closest:()=>form}});
 const saved:unknown[]=[],changed=vi.fn();view.addEventListener("erpx:record-command-saved",event=>saved.push((event as CustomEvent).detail));view.addEventListener("erpx:work-changed",changed);
 const fetcher=vi.fn<(input:RequestInfo|URL,init?:RequestInit)=>Promise<Response>>().mockResolvedValueOnce(new Response("truncated",{status:200})).mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));vi.stubGlobal("fetch",fetcher);
 const request=createFinanceCommandFetch(),url="/api/v1/holdings/acme/finance/calculation-runs",input={method:"POST",body:'{"kind":"cash_flow","input":{"profit":"10"}}'};
 expect((await request(url,input)).status).toBe(200);expect(saved).toEqual([]);expect(changed).not.toHaveBeenCalled();expect(request.hasAttempt(url,input.body)).toBe(true);
 const pending=request(url,input);fields="later-finance-edits";finish(Response.json({data:{id:"retained-calculation"}}));await pending;
 expect(saved).toEqual([{form,formEditVersion:"submitted-finance-fields",financeAcknowledged:true}]);expect(changed).toHaveBeenCalledTimes(1);expect(request.hasAttempt(url,input.body)).toBe(false);
 const keys=fetcher.mock.calls.map(call=>new Headers(call[1]?.headers).get("Idempotency-Key"));expect(keys[0]).toBe(keys[1]);
});

it('replaces a caller-provided assignment version with the original rendered authority',async()=>{
 const fetcher=vi.fn(async()=>Response.json({data:{saved:true}}));vi.stubGlobal('fetch',fetcher);
 await workFetch('/api/v1/holdings/acme/finance/journals',{method:'POST',headers:{'x-erpx-context-assignment-version':'999'}});
 expect(new Headers((fetcher.mock.calls[0] as unknown as [unknown,RequestInit])[1].headers).get('x-erpx-context-assignment-version')).toBe('3');
});

it('does not confuse distinct valid identifiers that share a colon-joined context string',async()=>{
 context.dataset={workUser:'a:7',workRevision:'3',workCompany:'c',workAssignment:'d',workAssignmentVersion:'1',workStale:'false'};
 let finish!:(value:Response)=>void;vi.stubGlobal('fetch',vi.fn(()=>new Promise<Response>(resolve=>{finish=resolve;})));
 const pending=workFetch('/api/v1/holdings/acme/people');
 context.dataset={workUser:'a',workRevision:'7',workCompany:'3',workAssignment:'c:d',workAssignmentVersion:'1',workStale:'false'};
 finish(Response.json({data:{privateDetail:'original synthetic account data'}}));await expectContextConflict(await pending);
});
