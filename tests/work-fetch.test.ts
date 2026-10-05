import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { workFetch } from "../src/lib/work-fetch";
import {
  announceContextSwitch,
  confirmContextSwitch,
  contextGeneration,
  contextIsSwitching,
  registerUnsavedWork,
  setContextSwitching,
} from "../src/lib/context-switch";

type ContextNode = { isConnected: boolean; dataset: Record<string, string> };
let context: ContextNode;
let platform: ContextNode;
let view: EventTarget & { location: { origin: string }; confirm: ReturnType<typeof vi.fn> };
let dirtyMarker: boolean;
let documentElement: { dataset: Record<string, string> };
let channels: { name: string; postMessage: ReturnType<typeof vi.fn>; close: ReturnType<typeof vi.fn> }[];
const subscriptions: (() => void)[] = [];

beforeEach(() => {
  context = { isConnected: true, dataset: { workUser: "user-a", workCompany: "company-a", workAssignment: "assignment-a", workRevision: "7", workStale: "false" } };
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
  const fetcher = vi.fn<typeof fetch>(async () => Response.json({ data: { selected: true } }));
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
  expect(headers.get("x-erpx-platform-assignment")).toBe("platform-a");
});

it.each(["/api/v1/holdings/acme/push", "/api/v1/holdings/acme/push/session", "/api/v1/front/acme/company/pms/push"])("allows Push cleanup during a pending switch at %s", async path => {
  const fetcher = vi.fn<typeof fetch>(async () => new Response(null, { status: 204 }));
  vi.stubGlobal("fetch", fetcher);
  setContextSwitching(true);
  expect((await workFetch(path, { method: "DELETE" })).status).toBe(204);
  expect(fetcher).toHaveBeenCalledTimes(1);
  const headers = new Headers((fetcher.mock.calls[0][1] as RequestInit).headers);
  expect(headers.get("x-erpx-context-company")).toBe("company-a");
  expect(headers.get("x-erpx-context-assignment")).toBe("assignment-a");
  expect(headers.get("x-erpx-context-revision")).toBe("7");
});

it("keeps an intentional Holding selector's caller headers and revision during a switch", async () => {
  const fetcher = vi.fn<typeof fetch>(async () => Response.json({ data: {} }));
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

it.each(["removed", "user", "company", "assignment", "revision", "stale"])("rejects a late response when rendered context is %s", async change => {
  let finish!: (value: Response) => void;
  vi.stubGlobal("fetch", vi.fn(() => new Promise<Response>(resolve => { finish = resolve; })));
  const pending = workFetch("/api/v1/holdings/acme/people");
  if (change === "removed") context.isConnected = false;
  else if (change === "stale") context.dataset.workStale = "true";
  else {
    const fields: Record<string, string> = { user: "workUser", company: "workCompany", assignment: "workAssignment", revision: "workRevision" };
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
