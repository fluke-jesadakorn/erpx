import type { ReactElement } from "react";
import {holdingFetch} from '@erpx/contracts/holding-fetch';
import {canonicalUiHref} from '@erpx/feature-work_projection/lib/destination-registry';
import {isProductHandoffHref} from '@erpx/feature-work_projection/lib/product-navigation';
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
vi.mock("@erpx/feature-work_projection/lib/work-fetch", () => ({ workFetch: vi.fn() }));
vi.mock("@erpx/feature-work_projection/lib/work-refresh", () => ({scheduleWorkRefresh: () => () => {}}));
vi.mock("@erpx/feature-work_projection/lib/context-switch", () => ({contextIsSwitching: () => false}));
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import { NavigationCountsProvider, useNavigationCounts } from "@erpx/workplace/components/navigation-counts";

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
vi.mock("@erpx/workplace/components/context-switch-provider",()=>({useContextSwitch:()=>({busy:false,switchContext:async()=>false})}));
import {WorkContextBoundary,type InitialWorkContextSnapshot} from '@erpx/workplace/components/work-context-boundary';
import type {ComponentProps} from 'react';
type BoundaryProps=ComponentProps<typeof WorkContextBoundary>;
const boundaryIdentity={base:'/holdings/acme',userId:'current-user',companyId:'current-company',assignmentId:'current-role',revision:7,assignmentVersion:3};
function boundarySnapshot(extra:Partial<InitialWorkContextSnapshot>={}):InitialWorkContextSnapshot{return {version:2,...boundaryIdentity,checkedAt:1_000_000,...extra};}
function boundaryData(extra:Record<string,unknown>={}){return {valid:true,...boundaryIdentity,...extra};}
function boundaryEventPort(){
 const handlers=new Map<string,Set<EventListenerOrEventListenerObject>>();
 return {addEventListener(name:string,handler:EventListenerOrEventListenerObject){const selected=handlers.get(name)??new Set();selected.add(handler);handlers.set(name,selected);},removeEventListener(name:string,handler:EventListenerOrEventListenerObject){handlers.get(name)?.delete(handler);},dispatchEvent(event:Event){for(const handler of handlers.get(event.type)??[]){if(typeof handler==='function')handler(event);else handler.handleEvent(event);}return true;},async emit(name:string){for(const handler of handlers.get(name)??[])await (typeof handler==='function'?handler(new Event(name)):handler.handleEvent(new Event(name)));},count(name:string){return handlers.get(name)?.size??0;}};
}
function mountedWorkBoundary(input:Partial<BoundaryProps>={},response:()=>Promise<Response>=async()=>Response.json({data:boundaryData()}),options:{deviceAccounts?:boolean;holding?:boolean}={}){
 vi.useFakeTimers({toFake:['Date']});vi.setSystemTime(1_000_100);
 const url=new URL('/holdings/acme/work?view=actions#current',options.holding?'https://acme.jesadakorn.com':'https://console-dev-erpx.jesadakorn.com');
 const browser={...boundaryEventPort(),location:{host:url.host,href:url.href,origin:url.origin,pathname:url.pathname,search:url.search,hash:url.hash,replace:vi.fn()}},channels:Array<ReturnType<typeof boundaryEventPort>&{closed:boolean}>=[];
 vi.stubGlobal('document',{documentElement:{dataset:{deviceAccounts:String(options.deviceAccounts??false),erpxApp:options.holding?'finance':'apps'}}});
 vi.stubGlobal('window',browser);vi.stubGlobal('BroadcastChannel',class{private readonly port=boundaryEventPort();closed=false;constructor(){channels.push(this);}addEventListener=this.port.addEventListener;removeEventListener=this.port.removeEventListener;dispatchEvent=this.port.dispatchEvent;emit=this.port.emit;count=this.port.count;close(){this.closed=true;}});
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
 {name:'different assignment version',snapshot:boundarySnapshot({assignmentVersion:4})},
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

it('keeps an actual Work purchase request title when product navigation is held and the user keeps editing',async()=>{
 const {readFile}=await import('node:fs/promises'),ts=await import('typescript'),{createRequire}=await import('node:module'),require=createRequire(import.meta.url),React=await import('react'),jsx=require('react/jsx-runtime') as {jsx:(type:unknown,props:Record<string,unknown>,key?:unknown)=>React.ReactElement;jsxs:(type:unknown,props:Record<string,unknown>,key?:unknown)=>React.ReactElement},
  stores=new WeakMap<(props:Record<string,unknown>)=>React.ReactNode,{slots:unknown[];cursor:number;effects:Map<number,{dependencies?:unknown[];cleanup?:()=>void}>}>(),allStores=new Set<{slots:unknown[];cursor:number;effects:Map<number,{dependencies?:unknown[];cleanup?:()=>void}>}>(),pendingEffects:(()=>void)[]=[],router={push:vi.fn(),replace:vi.fn(),refresh:vi.fn()},
  intakeOptions={nodes:[],branchId:null,departmentId:null,companyId:'current-company',userId:'current-user',currency:'THB'},calls:{url:string;method:string|undefined}[]=[],workFetch=vi.fn(async(url:string,init?:RequestInit)=>{calls.push({url,method:init?.method});return Response.json({data:{kind:'purchase_request',options:intakeOptions}});}),
  createAttempt=vi.fn(),upload=vi.fn(),send=vi.fn(),
  useRecoverableDraft=(_key:string,initial:Record<string,string>)=>{const [fields,setFields]=reactHooks.useState(initial);return{fields,update:(change:(previous:Record<string,string>)=>Record<string,string>)=>setFields(change),receiveAi:vi.fn(),clear:vi.fn(),discard:vi.fn(),accept:vi.fn(),dirty:JSON.stringify(fields)!==JSON.stringify(initial),conflict:false,notice:null};};
 let current:{slots:unknown[];cursor:number;effects:Map<number,{dependencies?:unknown[];cleanup?:()=>void}>}|null=null;
 const store=()=>{if(!current)throw Error('Hook rendered outside an actual component');return current;},
  useState=(initial:unknown)=>{const selected=store(),index=selected.cursor++;if(!(index in selected.slots))selected.slots[index]=typeof initial==='function'?(initial as()=>unknown)():initial;return[selected.slots[index],(next:unknown)=>{selected.slots[index]=typeof next==='function'?(next as(value:unknown)=>unknown)(selected.slots[index]):next;}] as const;},
  useRef=(initial:unknown)=>{const selected=store(),index=selected.cursor++;if(!(index in selected.slots))selected.slots[index]={current:initial};return selected.slots[index] as {current:unknown};},
  useMemo=(make:()=>unknown,dependencies?:unknown[])=>{const selected=store(),index=selected.cursor++,previous=selected.slots[index] as {dependencies?:unknown[];value:unknown}|undefined;if(!previous||!dependencies||dependencies.some((value,at)=>value!==previous.dependencies?.[at]))selected.slots[index]={dependencies,value:make()};return(selected.slots[index] as {value:unknown}).value;},
  useEffect=(run:()=>void|(()=>void),dependencies?:unknown[])=>{const selected=store(),index=selected.cursor++,previous=selected.effects.get(index);if(!previous||!dependencies||dependencies.some((value,at)=>value!==previous.dependencies?.[at]))pendingEffects.push(()=>{previous?.cleanup?.();selected.effects.set(index,{dependencies,cleanup:run()||undefined});});},
  useCallback=(fn:unknown)=>{store().cursor++;return fn;},
  reactHooks={...React,useState,useRef,useMemo,useCallback,useEffect,useLayoutEffect:useEffect,useTransition:()=>[false,(run:()=>void)=>run()] as const};
 const primitive=(tag:string)=>function WorkTestPrimitive(props:Record<string,unknown>){return jsx.jsx(tag,props);},
  controls={Button:primitive('button'),Card:primitive('section'),Field:primitive('label'),InlineNotice:primitive('p'),Select:primitive('select'),TextArea:primitive('textarea'),TextInput:primitive('input')},
  buttons={Button:controls.Button},dynamicCount={value:0},
  newWorkLink=(props:Record<string,unknown>)=>jsx.jsx('a',{href:props.href,'aria-label':props['aria-label'],onClick:props.onClick,children:props.children}),
  newWorkDialog=(props:Record<string,unknown>)=>props.title==='New work'&&!props.open?null:jsx.jsx('aside',props),
  drawer=(props:Record<string,unknown>)=>props.open?jsx.jsx('aside',props):null,
  contextPort={busy:false,switchContext:vi.fn()},
  contextSwitch={contextSwitchBlockReason:()=>null,hasUnsavedWork:()=>document.querySelector('[data-work-dirty="true"]')!==null,setContextSwitching:vi.fn()},
  compile=async(path:string,exportName:string,dependencies:(name:string)=>unknown)=>{
   const text=await readFile(new URL(`../../../${path}`,import.meta.url),'utf8'),code=ts.transpileModule(text,{compilerOptions:{esModuleInterop: false, jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022}}).outputText,loadedModule={exports:{} as Record<string,unknown>};
   new Function('require','module','exports',code)((name:string)=>name==='react'?reactHooks:name==='react/jsx-runtime'?require(name):dependencies(name),loadedModule,loadedModule.exports);return loadedModule.exports[exportName] as (props:Record<string,unknown>)=>React.ReactNode;
  };
 const requestEditor=await compile('packages/workplace/src/components/request-controls-content.tsx','RequestEditor',name=>{
  if(name.endsWith("@erpx/ui/components/foundation"))return controls;
  if(name.endsWith("@erpx/workplace/components/request-validation"))return{RequestValidation:({children}:{children:React.ReactNode})=>children};
  if(name.endsWith("@erpx/feature-work_projection/lib/request-draft-create"))return{createRequestDraftOnce:()=>createAttempt,createRequestUploader:()=>upload};
  if(name.endsWith("@erpx/workplace/components/review-decision"))return{ReviewDecisionForm:()=>null};
  if(name.endsWith("@erpx/workplace/components/request-experience"))return{RequestSteps:()=>null,RoutePreview:()=>null};
  if(name.endsWith("@erpx/feature-work_projection/lib/destination-registry"))return{canonicalUiHref};
  if(name.endsWith("@erpx/feature-work_projection/lib/work-fetch"))return{workFetch};
  if(name.endsWith("@erpx/workplace/components/workspace-link"))return{default:newWorkLink};
  if(name.endsWith("@erpx/feature-shared/lib/destination-router"))return{useRouter:()=>router};
  if(name.endsWith("@erpx/ui/components/use-hydrated"))return{useHydrated:()=>true};
  if(name.endsWith("@erpx/feature-shared/components/use-recoverable-draft"))return{useRecoverableDraft};
  if(name.endsWith('/guided-intake'))return{clearGuidedIntakeHandoff:vi.fn(),readGuidedIntakeHandoff:()=>null};
  if(name.endsWith("@erpx/feature-people_payroll/components/time/time-preference-context"))return{TimeText:()=>null};
  if(name.endsWith("@erpx/feature-work_projection/components/use-staff-command"))return{useStaffCommand:()=>({send,busy:false,error:''})};
  if(name.endsWith("@erpx/feature-people_payroll/lib/employee-services"))return{serviceCategories:[],requestStateLabels:{}};
  if(name.endsWith('.module.css'))return{default:new Proxy({},{get:(_target,key)=>String(key)})};
  throw Error('Unexpected actual RequestEditor import '+name);
 });
 const NewWork=await compile('packages/workplace/src/components/new-work.tsx','NewWork',name=>{
  if(name.endsWith("@erpx/ui/components/foundation"))return controls;
  if(name.endsWith("@erpx/ui/components/button"))return buttons;
  if(name==='next/dynamic')return{default:()=>++dynamicCount.value===3?requestEditor:()=>null};
  if(name.endsWith("@erpx/workplace/components/workspace-link"))return{default:newWorkLink};
  if(name.endsWith("@erpx/feature-work_projection/lib/work-fetch"))return{workFetch};
  if(name.endsWith("@erpx/ui/components/dialog"))return{Dialog:newWorkDialog};
  if(name.endsWith('/work-cards'))return{newWorkHref:(base:string)=>base+'/requests/new?kind=purchase'};
  if(name.endsWith("@erpx/feature-work_projection/lib/request-actions"))return{requestActions:{purchase_request:{label:'Purchase request',objective:'Describe the item.'}}};
  if(name.endsWith("@erpx/ui/components/use-hydrated"))return{useHydrated:()=>true};
  if(name.endsWith("@erpx/ui/components/draft-form-drawer"))return{DraftFormDrawer:drawer};
  if(name.endsWith('.module.css'))return{default:{}};
  if(name.endsWith("@erpx/feature-work_projection/lib/destination-registry"))return{canonicalUiHref};
  throw Error('Unexpected actual NewWork import '+name);
 });
 const WorkBoundary=await compile('packages/workplace/src/components/work-context-boundary.tsx','WorkContextBoundary',name=>{
  if(name==='@erpx/contracts/holding-fetch')return{holdingFetch};
  if(name.endsWith("@erpx/ui/components/foundation"))return controls;
  if(name.endsWith("@erpx/workplace/components/context-switch-provider"))return{useContextSwitch:()=>contextPort};
  throw Error('Unexpected actual WorkContextBoundary import '+name);
 });
 const ProductBoundary=await compile('packages/workplace/src/components/product-navigation-boundary.tsx','ProductNavigationBoundary',name=>{
  if(name.endsWith("@erpx/feature-work_projection/lib/product-navigation"))return{isProductHandoffHref};
  if(name==='next/navigation')return{useRouter:()=>router};
  if(name.endsWith("@erpx/feature-work_projection/lib/context-switch"))return contextSwitch;
  if(name.endsWith("@erpx/ui/components/dialog"))return{Dialog:newWorkDialog};
  if(name.endsWith("@erpx/ui/components/button"))return buttons;
  throw Error('Unexpected actual ProductNavigationBoundary import '+name);
 });
 const attributes=new Map<string,string>(),rootListeners=new Map<string,Set<(event:{target:unknown})=>void>>(),root={
  setAttribute:(name:string,value:string)=>attributes.set(name,value),removeAttribute:(name:string)=>attributes.delete(name),contains:()=>true,
  addEventListener:(name:string,listener:(event:{target:unknown})=>void)=>{const listeners=rootListeners.get(name)??new Set();listeners.add(listener);rootListeners.set(name,listeners);},
  removeEventListener:(name:string,listener:(event:{target:unknown})=>void)=>rootListeners.get(name)?.delete(listener),
 },formAttributes=new Map<string,string>(),form={isConnected:true,getAttribute:(name:string)=>formAttributes.get(name)??null,setAttribute:(name:string,value:string)=>formAttributes.set(name,value)},
  location={origin:'https://work.example.test',pathname:'/holdings/acme/work',search:'',hash:'',assign:vi.fn(),replace:vi.fn()},browser=new EventTarget();Object.assign(browser,{location});
 vi.stubGlobal('window',browser);vi.stubGlobal('HTMLElement',class{});vi.stubGlobal('document',{visibilityState:'visible',documentElement:{dataset:{}},activeElement:null,querySelector:(selector:string)=>selector==='[data-work-dirty="true"]'&&attributes.get('data-work-dirty')==='true'?root:null});
 vi.stubGlobal('sessionStorage',{getItem:()=>null,setItem:vi.fn(),removeItem:vi.fn()});vi.stubGlobal('MutationObserver',class{observe(){}disconnect(){}});vi.stubGlobal('BroadcastChannel',class{addEventListener(){}close(){}});
 const creation=[{kind:'purchase_request',label:'Purchase request',href:'/holdings/acme/requests/new?kind=purchase'}],workOptions={base:'/holdings/acme',userId:'current-user',companyId:'current-company',assignmentId:'current-role',revision:7,assignmentVersion:3,initialCurrent:{version:2,base:'/holdings/acme',userId:'current-user',companyId:'current-company',assignmentId:'current-role',revision:7,assignmentVersion:3,checkedAt:Date.now()}},
  children=React.createElement(React.Fragment,null,React.createElement(NewWork,{base:'/holdings/acme',creation,current:'/holdings/acme/work'}),React.createElement('a',{href:'https://finance-dev-erpx.jesadakorn.com/sso/start','data-product-switch':true},'Finance')),
  page=React.createElement(WorkBoundary,workOptions,children),tree=React.createElement(ProductBoundary,null,page);
 type TestNode=React.ReactElement<Record<string,unknown>>;
 const render=()=>{const nodes:TestNode[]=[];function visit(value:React.ReactNode){React.Children.forEach(value,child=>{if(!React.isValidElement<Record<string,unknown>>(child))return;if(typeof child.type==='function'){const component=child.type as(props:Record<string,unknown>)=>React.ReactNode,selected=stores.get(component)??{slots:[],cursor:0,effects:new Map()};stores.set(component,selected);allStores.add(selected);selected.cursor=0;const previous=current;current=selected;let output:React.ReactNode;try{output=component(child.props);}finally{current=previous;}visit(output);return;}nodes.push(child);if(child.type==='div'&&child.props.ref&&typeof child.props.ref==='object')((child.props.ref as{current:unknown}).current)=root;if(child.type==='form')Object.assign(form,{isConnected:true});visit(child.props.children as React.ReactNode);});}visit(tree);for(const run of pendingEffects.splice(0))run();return nodes;},
  find=(nodes:TestNode[],predicate:(node:TestNode)=>boolean)=>{const node=nodes.find(predicate);if(!node)throw Error('Missing mounted Work node');return node;};
 try{
  let nodes=render();const newButton=find(nodes,node=>node.type==='button'&&node.props.children==='New');(newButton.props.onClick as()=>void)();nodes=render();
  const purchaseLink=find(nodes,node=>node.type==='a'&&String(node.props.href).includes('/requests/new'));(purchaseLink.props.onClick as(event:{preventDefault:()=>void;defaultPrevented?:boolean})=>void)({preventDefault:vi.fn(),defaultPrevented:false});
  await new Promise<void>(resolve=>setImmediate(resolve));expect(calls).toEqual([{url:'/api/v1/holdings/acme/work/intake?kind=purchase_request',method:undefined}]);
  nodes=render();const title=find(nodes,node=>node.type==='input'&&node.props.placeholder==='Uses the first item'),titleInput=title.props.onChange as(event:{target:{value:string}})=>void;titleInput({target:{value:'Unsaved purchase request'}});
  const formNode=find(nodes,node=>node.type==='form'),inputTarget={matches:()=>false,closest:(selector:string)=>selector==='form'?form:null};for(const listener of rootListeners.get('input')??[])listener({target:inputTarget});
  nodes=render();const heldTitle=find(nodes,node=>node.type==='input'&&node.props.placeholder==='Uses the first item');expect(heldTitle.props.value).toBe('Unsaved purchase request');expect(attributes.get('data-work-dirty')).toBe('true');
  const boundary=find(nodes,node=>node.type==='div'&&typeof node.props.onClickCapture==='function'),destination={href:'https://finance-dev-erpx.jesadakorn.com/sso/start',target:''},click={button:0,metaKey:false,ctrlKey:false,shiftKey:false,altKey:false,target:{closest:()=>destination},preventDefault:vi.fn(),stopPropagation:vi.fn()};
  (boundary.props.onClickCapture as(event:typeof click)=>void)(click);expect(click.preventDefault).toHaveBeenCalledOnce();nodes=render();expect(find(nodes,node=>node.type==='aside'&&node.props.title==='Leave unsaved work?').props.open).toBe(true);expect(location.assign).not.toHaveBeenCalled();
  (find(nodes,node=>node.type==='button'&&node.props.children==='Keep editing').props.onClick as()=>void)();nodes=render();expect(find(nodes,node=>node.type==='input'&&node.props.placeholder==='Uses the first item').props.value).toBe('Unsaved purchase request');expect(find(nodes,node=>node.type==='aside'&&node.props.title==='Leave unsaved work?').props.open).toBe(false);
  expect(calls).toHaveLength(1);expect(send).not.toHaveBeenCalled();expect(createAttempt).not.toHaveBeenCalled();expect(upload).not.toHaveBeenCalled();expect(router.push).not.toHaveBeenCalled();expect(router.replace).not.toHaveBeenCalled();expect(router.refresh).not.toHaveBeenCalled();expect(location.assign).not.toHaveBeenCalled();expect(formNode.type).toBe('form');
 }finally{for(const selected of allStores)for(const effect of selected.effects.values())effect.cleanup?.();vi.unstubAllGlobals();}
});

it('keeps Account-aware401 recovery separate and gives every mounted boundary its own browser metadata',async()=>{
 const account=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>new Response(null,{status:401}),{deviceAccounts:true});
 const stale=vi.fn();account.browser.addEventListener('erpx:device-account-stale',stale);await account.browser.emit('focus');expect(stale).toHaveBeenCalledOnce();expect(account.browser.location.replace).not.toHaveBeenCalled();account.unmount();
 const ordinary=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>new Response(null,{status:401}));await ordinary.browser.emit('focus');expect(ordinary.browser.location.replace).toHaveBeenCalledExactlyOnceWith('/login?reason=expired&next='+encodeURIComponent('/holdings/acme/work?view=actions#current'));expect(stale).toHaveBeenCalledTimes(1);ordinary.unmount();
});
it('uses the real registered Holding transport while retaining Company headers and current-authority denial',async()=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>new Response(null,{status:403}),{holding:true});expect(view.fetcher).not.toHaveBeenCalled();await view.browser.emit('focus');
 expect(view.fetcher).toHaveBeenCalledExactlyOnceWith(new URL('https://acme.jesadakorn.com/__erpx/finance/api/v1/holdings/acme/assignments/active'),{cache:'no-store',headers:{'x-erpx-context-company':'current-company'}});expect(view.stale()).toBe('true');expect(view.browser.location.replace).not.toHaveBeenCalled();view.unmount();
});

it('marks the rendered role stale when only its assignment version changes',async()=>{
 const view=mountedWorkBoundary({initialCurrent:boundarySnapshot()},async()=>Response.json({data:boundaryData({assignmentVersion:4})}));
 expect(view.fetcher).not.toHaveBeenCalled();await view.browser.emit('focus');expect(view.stale()).toBe('true');view.unmount();
});
