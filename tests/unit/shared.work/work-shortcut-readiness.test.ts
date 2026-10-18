import type { ReactElement } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Drive the real provider's state and request callbacks without a DOM or server.
const hooks = vi.hoisted(() => ({
  used: [] as unknown[],
  resolved: null as unknown,
  slots: [] as { value: unknown }[],
  cursor: 0,
  context: null as unknown,
  effects: [] as (() => void | (() => void))[],
  pathname: "/holdings/acme/work",
}));
vi.mock("react", async (original) => ({
  ...await original<typeof import("react")>(),
  useState<T>(initial: T | (() => T)) {
    const index = hooks.cursor++;
    hooks.slots[index] ??= { value: typeof initial === "function" ? (initial as () => T)() : initial };
    return [hooks.slots[index].value as T, (update: T | ((value: T) => T)) => {
      hooks.slots[index].value = typeof update === "function"
        ? (update as (value: T) => T)(hooks.slots[index].value as T) : update;
    }];
  },
  useRef<T>(initial: T) {
    const index = hooks.cursor++;
    hooks.slots[index] ??= { value: { current: initial } };
    return hooks.slots[index].value;
  },
  useCallback<T>(callback: T) { return callback; },
  useContext() { return hooks.context; },
  use(value:unknown) {hooks.used.push(value);return hooks.resolved;},
  useEffect(effect: () => void | (() => void)) { hooks.effects.push(effect); },
}));
vi.mock("next/navigation", () => ({
  usePathname: () => hooks.pathname,
  useSearchParams: () => new URLSearchParams(),
}));
vi.mock("../../../src/lib/work_projection/work-fetch", () => ({ workFetch: vi.fn() }));
vi.mock("../../../src/lib/work_projection/work-refresh", () => ({scheduleWorkRefresh: () => () => {}}));
vi.mock("../../../src/lib/work_projection/context-switch", () => ({contextIsSwitching: () => false}));
import { workFetch } from "../../../src/lib/work_projection/work-fetch";
import { NavigationCountsProvider, useNavigationCounts } from "../../../src/components/workplace/navigation-counts";

type Navigation = ReturnType<typeof useNavigationCounts>;

const fetchMock = vi.mocked(workFetch);
function render() {
  hooks.cursor = 0;
  const provider = NavigationCountsProvider({ base: "/holdings/acme", shortcutStorageKey: "acme", children: null });
  return (provider as ReactElement<{ value: Navigation }>).props.value;
}
beforeEach(() => {
  hooks.used = [];
  hooks.resolved = null;
  hooks.slots = [];
  hooks.effects = [];
  hooks.cursor = 0;
  fetchMock.mockReset();
});

describe("Navigation counts without shortcuts", () => {
  it("loads counts without requiring a shortcut projection", async () => {
    fetchMock.mockResolvedValueOnce(Response.json({ data: { home: 2, work: 3, workViewAvailability: { team: true, routing: false } } }));
    await render().refresh();
    expect(render().counts).toEqual({ home: 2, work: 3 });
    expect(render().workViewAvailability).toEqual({ team: true, routing: false });
  });

  it("preserves a published work count when an older refresh resolves", async () => {
    let resolve!: (response: Response) => void;
    fetchMock.mockReturnValueOnce(new Promise<Response>(done => { resolve = done; }));
    const pending = render().refresh();
    render().publishWork(5);
    resolve(Response.json({ data: { home: 2, work: 3 } }));
    await pending;
    expect(render().counts).toEqual({ home: 2, work: 5 });
  });

  it("retains the published count after a failed refresh", async () => {
    render().publishWork(5);
    fetchMock.mockRejectedValueOnce(Error("network unavailable"));
    await render().refresh();
    expect(render().counts).toEqual({ home: null, work: 5 });
    expect(render().error).toBe(true);
  });
});


const attentionData = {home:2,work:7,attention:{items:[],total:7,waiting:4,incomplete:false},snapshotAt:100};
afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); });

it("coalesces simultaneous Home/menu refreshes into one attention request", async () => {
  let resolve!: (response:Response) => void;
  fetchMock.mockReturnValueOnce(new Promise<Response>(done => {resolve=done;}));
  const navigation = render();
  const pending = Promise.all([navigation.refresh(),navigation.refresh(),navigation.refresh()]);
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(fetchMock.mock.calls[0][0]).toBe("/api/v1/holdings/acme/navigation?include=attention");
  resolve(Response.json({data:attentionData})); await pending;
  expect(render().attention).toEqual(attentionData.attention);
  expect(render().counts.work).toBe(7);
});

it("retains attention and its timestamp during refresh and failure, then recovers on retry", async () => {
  fetchMock.mockResolvedValueOnce(Response.json({data:attentionData}));
  await render().refresh();
  let reject!: (error:Error) => void;
  fetchMock.mockReturnValueOnce(new Promise<Response>((_,fail) => {reject=fail;}));
  const pending = render().refresh();
  expect(render().updating).toBe(true);
  expect(render().attention).toEqual(attentionData.attention);
  reject(Error("offline")); await pending;
  expect(render().snapshotAt).toBe(100);
  expect(render().attention.total).toBe(7);
  expect(render().error).toBe(true);
  fetchMock.mockResolvedValueOnce(Response.json({data:{...attentionData,work:8,snapshotAt:200}}));
  await render().refresh();
  expect(render().error).toBe(false);
  expect(render().counts.work).toBe(8);
});

it.each([401,403,404,409])("clears private attention after access response %s", async status => {
  fetchMock.mockResolvedValueOnce(Response.json({data:attentionData})); await render().refresh();
  fetchMock.mockResolvedValueOnce(Response.json({error:{}},{status})); await render().refresh();
  expect(render().counts).toEqual({home:null,work:null});
  expect(render().attention.items).toEqual([]);
  expect(render().snapshotAt).toBe(0);
});

it.each(["erpx:context-switch-started","erpx:work-context-changed"])("discards responses immediately after %s", async event => {
  vi.useFakeTimers(); vi.stubGlobal("window",new EventTarget());
  const navigation = render(); const cleanup = hooks.effects[0]();
  fetchMock.mockResolvedValueOnce(Response.json({data:attentionData})); await navigation.refresh();
  let resolve!: (response:Response) => void;
  fetchMock.mockReturnValueOnce(new Promise<Response>(done => {resolve=done;}));
  const pending = render().refresh();
  window.dispatchEvent(new Event(event));
  expect(render().counts.work).toBeNull();
  expect(render().snapshotAt).toBe(0);
  resolve(Response.json({data:attentionData})); await pending;
  render().publishWork(99);
  expect(render().counts.work).toBeNull();
  expect(render().attention.total).toBe(0);
  if (cleanup) cleanup();
});


describe("streamed badge retry",()=>{
 it("consumes its initial promise after the provider commits without replacing newer counts",()=>{
  const initial=Promise.resolve(null),state=render();
  hooks.resolved={home:3,work:4};
  hooks.context={...state,initial,initialPending:true};
  expect(useNavigationCounts().counts).toEqual({home:3,work:4});
  hooks.context={...state,initial,initialPending:false,counts:{home:7,work:9}};
  expect(useNavigationCounts().counts).toEqual({home:7,work:9});
  expect(hooks.used).toEqual([initial,initial]);
 });
});


// Work-context hydration uses its actual hook/effect body. Network, clock and
// browser event ports are controlled; no server authority is fabricated here.
vi.mock('../../../src/components/workplace/context-switch-provider',()=>({useContextSwitch:()=>({busy:false,switchContext:async()=>false})}));
import {WorkContextBoundary,type InitialWorkContextSnapshot} from '../../../src/components/workplace/work-context-boundary';
import type {ComponentProps} from 'react';
type BoundaryProps=ComponentProps<typeof WorkContextBoundary>;
const boundaryIdentity={base:'/holdings/acme',userId:'current-user',companyId:'current-company',assignmentId:'current-role',revision:7};
function boundarySnapshot(extra:Partial<InitialWorkContextSnapshot>={}):InitialWorkContextSnapshot{return {version:1,...boundaryIdentity,checkedAt:1_000_000,...extra};}
function boundaryData(extra:Record<string,unknown>={}){return {valid:true,...boundaryIdentity,...extra};}
function boundaryEventPort(){
 const handlers=new Map<string,Set<EventListenerOrEventListenerObject>>();
 return {addEventListener(name:string,handler:EventListenerOrEventListenerObject){const selected=handlers.get(name)??new Set();selected.add(handler);handlers.set(name,selected);},removeEventListener(name:string,handler:EventListenerOrEventListenerObject){handlers.get(name)?.delete(handler);},async emit(name:string){for(const handler of handlers.get(name)??[])await (typeof handler==='function'?handler(new Event(name)):handler.handleEvent(new Event(name)));},count(name:string){return handlers.get(name)?.size??0;}};
}
function mountedWorkBoundary(input:Partial<BoundaryProps>={},response:()=>Promise<Response>=async()=>Response.json({data:boundaryData()})){
 vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(1_000_100);
 const browser={...boundaryEventPort(),location:{pathname:'/holdings/acme/work',search:'?view=actions',hash:'#current',replace:vi.fn()}},channels:Array<ReturnType<typeof boundaryEventPort>&{closed:boolean}>=[];
 vi.stubGlobal('window',browser);vi.stubGlobal('BroadcastChannel',class{private readonly port=boundaryEventPort();closed=false;constructor(){channels.push(this);}addEventListener=this.port.addEventListener;removeEventListener=this.port.removeEventListener;emit=this.port.emit;count=this.port.count;close(){this.closed=true;}});
 vi.stubGlobal('MutationObserver',class{observe(){}disconnect(){}});const fetcher=vi.fn<(...args:Parameters<typeof fetch>)=>ReturnType<typeof fetch>>(response);vi.stubGlobal('fetch',fetcher);
 let props:BoundaryProps={...boundaryIdentity,children:null,...input},cleanup:(()=>void)|undefined;
 const node={setAttribute:vi.fn(),removeAttribute:vi.fn(),contains:()=>false,addEventListener:vi.fn(),removeEventListener:vi.fn()};
 function render(){hooks.cursor=0;hooks.effects=[];const element=WorkContextBoundary(props) as ReactElement<{ref:{current:unknown};'data-work-stale':string;'data-work-user':string}>;element.props.ref.current=node;return element;}
 function start(){render();const effect=hooks.effects[0];if(!effect)throw Error('Actual Work context effect missing');const result=effect();cleanup=typeof result==='function'?result:undefined;}
 start();return {browser,channels,fetcher,render,update(next:Partial<BoundaryProps>){cleanup?.();props={...props,...next};start();},unmount(){cleanup?.();},stale:()=>render().props['data-work-stale']};
}
it.each(['workspace','front'] as const)('uses a matching fresh %s SSR identity without a duplicate mount request',async kind=>{
 const front=kind==='front'?{frontApp:'pos' as const,frontLocation:'branches/north',frontRoleEndpoint:'/api/v1/front/acme/company/pos'}:{},view=mountedWorkBoundary({...front,initialCurrent:boundarySnapshot(front)});
 expect(view.fetcher).not.toHaveBeenCalled();expect(view.stale()).toBe('false');expect(view.browser.count('focus')).toBe(1);expect(view.channels).toHaveLength(1);view.unmount();expect(view.channels[0].closed).toBe(true);
});
it.each([
 {name:'absent',snapshot:undefined},
 {name:'different base',snapshot:boundarySnapshot({base:'/holdings/other'})},
 {name:'different account',snapshot:boundarySnapshot({userId:'other-user'})},
 {name:'different Company',snapshot:boundarySnapshot({companyId:'other-company'})},
 {name:'different assignment',snapshot:boundarySnapshot({assignmentId:'other-role'})},
 {name:'different revision',snapshot:boundarySnapshot({revision:8})},
 {name:'expired',snapshot:boundarySnapshot({checkedAt:969_999})},
 {name:'future clock',snapshot:boundarySnapshot({checkedAt:1_000_101})},
 {name:'wrong Front endpoint',snapshot:boundarySnapshot({frontRoleEndpoint:'/api/v1/front/acme/other/pos'})},
 {name:'wrong Front location',snapshot:boundarySnapshot({frontLocation:'branches/other'})},
] as const)('keeps the fresh native mount read for a $name snapshot',entry=>{
 const view=mountedWorkBoundary({initialCurrent:entry.snapshot});expect(view.fetcher).toHaveBeenCalledExactlyOnceWith('/api/v1/holdings/acme/assignments/active',{cache:'no-store',headers:{'x-erpx-context-company':'current-company'}});view.unmount();
});
it.each(['focus','erpx:work-context-changed','broadcast'] as const)('always revalidates on %s after a matched SSR seed and retains revocation denial',async event=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>Response.json({error:{code:'WORK_ROLE_UNAVAILABLE'}},{status:403}));expect(view.fetcher).not.toHaveBeenCalled();
 if(event==='broadcast')await view.channels[0].emit('message');else await view.browser.emit(event);
 expect(view.fetcher).toHaveBeenCalledTimes(1);expect(view.stale()).toBe('true');view.unmount();
});
it('checks a switched account in the actual Front original JSON and keeps the native location header',async()=>{
 const front={frontApp:'pms' as const,frontLocation:'branches/north',frontRoleEndpoint:'/api/v1/front/acme/company/pms'},view=mountedWorkBoundary({...front,initialCurrent:boundarySnapshot(front)},async()=>Response.json({data:boundaryData({userId:'other-account'})}));
 expect(view.fetcher).not.toHaveBeenCalled();await view.browser.emit('focus');expect(view.fetcher).toHaveBeenCalledExactlyOnceWith(front.frontRoleEndpoint+'/roles',{cache:'no-store',headers:{'x-erpx-front-location':front.frontLocation}});expect(view.stale()).toBe('true');view.unmount();
});
it('does not let a delayed original JSON invalidate a newer mounted user/source after effect cleanup',async()=>{
 let reading!:()=>void,release!:()=>void;const reached=new Promise<void>(done=>{reading=done;}),gate=new Promise<void>(done=>{release=done;});let first=true;
 class LateOriginalJSON extends Response{override async json(){const value=await super.json();reading();await gate;return value;}}
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>{if(first){first=false;return new LateOriginalJSON(JSON.stringify({data:boundaryData({valid:false,userId:'old-account',revision:99})}),{headers:{'Content-Type':'application/json'}});}return Response.json({data:boundaryData({userId:'new-account',revision:8})});});
 const pending=view.browser.emit('focus');await reached;view.update({userId:'new-account',revision:8,initialCurrent:boundarySnapshot({userId:'new-account',revision:8})});await view.browser.emit('focus');expect(view.render().props['data-work-user']).toBe('new-account');expect(view.stale()).toBe('false');release();await pending;expect(view.stale()).toBe('false');view.unmount();
});
it('retains the normal expired-session redirect after initial SSR deduplication',async()=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>new Response(null,{status:401}));await view.browser.emit('focus');expect(view.browser.location.replace).toHaveBeenCalledExactlyOnceWith('/login?reason=expired&next='+encodeURIComponent('/holdings/acme/work?view=actions#current'));view.unmount();
});

it('keeps an unsnapshotted legacy Front boundary on the original mount-role read',()=>{
 const view=mountedWorkBoundary({userId:undefined,frontApp:'pos',frontLocation:'holding',frontRoleEndpoint:'/api/v1/front/acme/company/pos'});expect(view.fetcher).toHaveBeenCalledExactlyOnceWith('/api/v1/front/acme/company/pos/roles',{cache:'no-store',headers:{'x-erpx-front-location':'holding'}});view.unmount();
});


it.each(['user','base'] as const)('never reuses the original SSR snapshot after a %s roundtrip in the same mounted boundary',field=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()});expect(view.fetcher).not.toHaveBeenCalled();
 view.update(field==='user'?{userId:'another-account'}:{base:'/holdings/other'});expect(view.fetcher).toHaveBeenCalledTimes(1);
 view.update({...boundaryIdentity,initialCurrent:boundarySnapshot()});expect(view.fetcher).toHaveBeenCalledTimes(2);expect(view.fetcher).toHaveBeenLastCalledWith('/api/v1/holdings/acme/assignments/active',{cache:'no-store',headers:{'x-erpx-context-company':'current-company'}});view.unmount();
});
it('never reuses the original Front snapshot after leaving and returning to its location',()=>{
 const front={frontApp:'pos' as const,frontLocation:'holding',frontRoleEndpoint:'/api/v1/front/acme/company/pos'},view=mountedWorkBoundary({...front,initialCurrent:boundarySnapshot(front)});expect(view.fetcher).not.toHaveBeenCalled();
 view.update({frontLocation:'branches/north'});expect(view.fetcher).toHaveBeenCalledExactlyOnceWith(front.frontRoleEndpoint+'/roles',{cache:'no-store',headers:{'x-erpx-front-location':'branches/north'}});
 view.update({...front,initialCurrent:boundarySnapshot(front)});expect(view.fetcher).toHaveBeenCalledTimes(2);expect(view.fetcher).toHaveBeenLastCalledWith(front.frontRoleEndpoint+'/roles',{cache:'no-store',headers:{'x-erpx-front-location':'holding'}});view.unmount();
});
it('still deduplicates StrictMode same-source effect cleanup/setup replay without duplicate listeners',()=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()});expect(view.fetcher).not.toHaveBeenCalled();
 // The actual effect is cleaned up and set up again with the same hook state
 // and identity, as in StrictMode. No source refusal occurred in between.
 view.update({});expect(view.fetcher).not.toHaveBeenCalled();expect(view.channels).toHaveLength(2);expect(view.channels[0].closed).toBe(true);expect(view.channels[1].closed).toBe(false);expect(view.browser.count('focus')).toBe(1);expect(view.browser.count('erpx:work-context-changed')).toBe(1);view.unmount();
});
it('keeps a refused expired snapshot ineligible even if the clock later makes its original timestamp look fresh',()=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot({checkedAt:969_999})});expect(view.fetcher).toHaveBeenCalledTimes(1);vi.setSystemTime(970_000);view.update({});expect(view.fetcher).toHaveBeenCalledTimes(2);view.unmount();
});
