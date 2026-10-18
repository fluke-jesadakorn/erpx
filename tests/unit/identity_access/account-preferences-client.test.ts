import { expect, it, vi } from "vitest";
import { createAccountPreferencesClient } from "@/lib/identity_access/account-preferences-client";
import type { AccountPreferences, AccountPreferenceSnapshot } from "@/server/modules/identity_access/public/account-preferences";

const key = "a".repeat(64), other = "b".repeat(64);
const preference = (version = 1, appearance: AccountPreferences["appearance"] = "light", language: AccountPreferences["language"] = "en") => ({ version, appearance, language });
const snapshot = (version = 1, appearance: AccountPreferences["appearance"] = "light", language: AccountPreferences["language"] = "en"): AccountPreferenceSnapshot => ({ key, preference: preference(version, appearance, language) });
function deferred<T>() {
  let resolve!: (value: T) => void, reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function settled() { for (let i = 0; i < 12; ++i) await Promise.resolve(); }
function setup(initial: AccountPreferenceSnapshot = snapshot()) {
  let generation: number | null = 0;
  const ports = { read: vi.fn(async (_signal: AbortSignal) => {void _signal;return snapshot();}),
    write: vi.fn(async (_key: string, version: number, values: Pick<AccountPreferences, "language" | "appearance">, _signal: AbortSignal): Promise<AccountPreferenceSnapshot> => {void _signal;return { key, preference: { ...values, version: version + 1 } };}),
    apply: vi.fn(), changed: vi.fn(), generation: () => generation };
  return { ports, client: createAccountPreferencesClient(initial, ports), generation: (next: number | null) => { generation = next; } };
}

it("uses matching server content without a mount or navigation GET and keeps an optional outage in the same account", () => {
  const { client, ports } = setup();
  client.observe(snapshot());
  client.observe({ key, preference: undefined });
  expect(client.getSnapshot()).toMatchObject({ key, preference: preference(), pending: false });
  expect(ports.read).not.toHaveBeenCalled(); expect(ports.write).not.toHaveBeenCalled();
  expect(ports.apply).not.toHaveBeenCalled();
});

it("serializes rapid theme/language edits with the acknowledged version and presents the latest intent", async () => {
  const { client, ports } = setup(), first = deferred<AccountPreferenceSnapshot>();
  ports.write.mockImplementationOnce(() => first.promise);
  client.change({ appearance: "dark" });
  client.change({ language: "th" });
  expect(ports.write).toHaveBeenCalledTimes(1);
  expect(client.getSnapshot().preference).toEqual(preference(1, "dark", "th"));
  first.resolve(snapshot(2, "dark")); await settled();
  expect(ports.write.mock.calls.map(call => call.slice(0, 3))).toEqual([
    [key, 1, { appearance: "dark", language: "en" }], [key, 2, { appearance: "dark", language: "th" }],
  ]);
  expect(client.getSnapshot()).toMatchObject({ preference: preference(3, "dark", "th"), pending: false, error: "" });
  expect(ports.read).not.toHaveBeenCalled();
});

it("does not apply a late write after the account/context changed, even if abort is ignored by the transport", async () => {
  const { client, ports, generation } = setup(), late = deferred<AccountPreferenceSnapshot>();
  ports.write.mockImplementationOnce(() => late.promise);
  client.change({ appearance: "dark" });
  const applications = ports.apply.mock.calls.length;
  generation(1); client.cancel();
  expect(ports.write.mock.calls[0][3].aborted).toBe(true);
  late.resolve(snapshot(2, "dark")); await settled();
  expect(ports.apply).toHaveBeenCalledTimes(applications);
  expect(ports.changed).not.toHaveBeenCalled();
  expect(client.getSnapshot()).toMatchObject({ preference: preference(), pending: false });
});

it("rejects a preference response for another account and never sends its version back with the rendered key", async () => {
  const { client, ports } = setup();
  ports.write.mockResolvedValueOnce({ key: other, preference: preference(50, "dark", "th") });
  ports.read.mockResolvedValueOnce({ key: other, preference: preference(50, "dark", "th") });
  client.change({ appearance: "dark" }); await settled();
  expect(client.getSnapshot().preference).toEqual(preference());
  expect(client.getSnapshot().error).toContain("account changed");
  expect(ports.apply.mock.calls.some(([next]) => next.version === 50)).toBe(false);
  expect(ports.write).toHaveBeenCalledTimes(1);
});

it("shows the winner of a conflicting edit and waits for an explicit retry before using its new version", async () => {
  const { client, ports } = setup();
  ports.write.mockRejectedValueOnce(new Error("Settings changed elsewhere."));
  ports.read.mockResolvedValueOnce(snapshot(2, "system", "th"));
  client.change({ appearance: "dark" }); await settled();
  expect(client.getSnapshot()).toMatchObject({ preference: preference(2, "system", "th"), error: "Settings changed elsewhere.", pending: false });
  expect(ports.write).toHaveBeenCalledTimes(1);
  client.retry(); await settled();
  expect(ports.write.mock.calls[1].slice(0, 3)).toEqual([key, 2, { appearance: "dark", language: "en" }]);
  expect(client.getSnapshot()).toMatchObject({ preference: preference(3, "dark"), error: "" });
});

it("recognizes a committed edit after a lost response instead of reporting a false failure or retrying it", async () => {
  const { client, ports } = setup();
  ports.write.mockRejectedValueOnce(new Error("Response lost."));
  ports.read.mockResolvedValueOnce(snapshot(2, "dark"));
  client.change({ appearance: "dark" }); await settled(); client.retry(); await settled();
  expect(client.getSnapshot()).toMatchObject({ preference: preference(2, "dark"), error: "", pending: false });
  expect(ports.write).toHaveBeenCalledTimes(1);
});

it("coalesces concurrent focus/peer reads and preserves an edit made while that read is pending", async () => {
  const { client, ports } = setup(), read = deferred<AccountPreferenceSnapshot>();
  ports.read.mockImplementationOnce(() => read.promise);
  const one = client.refresh(), two = client.refresh();
  expect(one).toBe(two); expect(ports.read).toHaveBeenCalledTimes(1);
  client.change({ appearance: "dark" });
  expect(ports.write).not.toHaveBeenCalled();
  read.resolve(snapshot(4, "light", "th")); await settled();
  expect(ports.write.mock.calls[0].slice(0, 3)).toEqual([key, 4, { appearance: "dark", language: "en" }]);
  expect(ports.apply.mock.calls.map(([next]) => next.appearance)).not.toContain("light");
  expect(client.getSnapshot()).toMatchObject({ preference: preference(5, "dark"), pending: false });
});

it("reads an unavailable optional seed only on an actual edit and keeps legacy Front language on first persistence", async () => {
  const { client, ports } = setup({ key, preference: undefined });
  expect(ports.read).not.toHaveBeenCalled();
  ports.read.mockResolvedValueOnce(snapshot(0, "system"));
  client.change({ appearance: "dark" }, { appearance: "light", language: "th" }); await settled();
  expect(ports.write.mock.calls[0].slice(0, 3)).toEqual([key, 0, { appearance: "dark", language: "th" }]);
  expect(client.getSnapshot().preference).toEqual(preference(1, "dark", "th"));
});

it("keeps unpersisted legacy choices during a focus refresh, and keeps anonymous choices browser-local", async () => {
  const { client, ports } = setup(snapshot(0));
  client.seedLegacy({ appearance: "light", language: "th" });
  ports.read.mockResolvedValueOnce(snapshot(0, "system")); await client.refresh();
  expect(client.getSnapshot().preference).toEqual(preference(0, "light", "th"));
  expect(ports.write).not.toHaveBeenCalled();
  const anonymous = setup({ key: "anonymous", preference: preference(0) });
  anonymous.client.change({ appearance: "dark" }); await anonymous.client.refresh();
  expect(anonymous.client.getSnapshot().preference).toEqual(preference(0, "dark"));
  expect(anonymous.ports.read).not.toHaveBeenCalled(); expect(anonymous.ports.write).not.toHaveBeenCalled();
});

it("accepts a matching central Account save without a duplicate PATCH and ignores older server snapshots", async () => {
  const { client, ports } = setup();
  client.receive(snapshot(3, "dark", "th"));
  client.observe(snapshot(2)); client.receive({ key: other, preference: preference(99) });
  expect(client.getSnapshot().preference).toEqual(preference(3, "dark", "th"));
  expect(ports.write).not.toHaveBeenCalled(); expect(ports.read).not.toHaveBeenCalled();
});

import {createAccountLanguageRefresh,createAccountPreferenceRefresh} from '@/lib/identity_access/account-preference-refresh';
import type {PreferenceRefreshOutcome} from '@/lib/identity_access/account-preferences-client';
function synchronizationFixture(){
 let visible=true,active=true,sequence=0;const timers=new Map<number,{run:()=>void;delay:number}>(),read=vi.fn(async():Promise<PreferenceRefreshOutcome>=>'ok');
 const synchronization=createAccountPreferenceRefresh({visible:()=>visible,active:()=>active,read,setTimer:(run,delay)=>{const id=++sequence;timers.set(id,{run,delay});return id;},clearTimer:id=>{timers.delete(id);}});
 return {synchronization,read,timers,visible:(next:boolean)=>{visible=next;},active:(next:boolean)=>{active=next;},fire:async()=>{const first=timers.entries().next().value;if(!first)throw Error('No scheduled revalidation');timers.delete(first[0]);first[1].run();await settled();},delay:()=>[...timers.values()][0]?.delay};
}
it('converges a second host through only its own read after the first quiet interval without a mount GET or a second PATCH',async()=>{
 let saved=snapshot(),firstReads=0,secondReads=0,writes=0;const first=createAccountPreferencesClient(saved,{read:async()=>{firstReads++;return saved;},write:async(_key,version,values)=>{writes++;return saved={key,preference:{...values,version:version+1}};},apply:vi.fn(),changed:vi.fn(),generation:()=>0}),second=createAccountPreferencesClient(saved,{read:async()=>{secondReads++;return saved;},write:async()=>{throw Error('A peer must never rewrite account preferences');},apply:vi.fn(),changed:vi.fn(),generation:()=>0}),timers=new Map<number,()=>void>();
 const synchronization=createAccountPreferenceRefresh({visible:()=>true,active:()=>true,read:()=>second.refresh(),setTimer:(run,delay)=>{expect(delay).toBe(30000);timers.set(1,run);return 1;},clearTimer:id=>{timers.delete(id);}});synchronization.start();expect(firstReads).toBe(0);expect(secondReads).toBe(0);first.change({appearance:'dark',language:'th'});await settled();expect(writes).toBe(1);expect(second.getSnapshot().preference).toEqual(preference());timers.get(1)!();await settled();expect(secondReads).toBe(1);expect(firstReads).toBe(0);expect(second.getSnapshot().preference).toEqual(preference(2,'dark','th'));expect(writes).toBe(1);synchronization.dispose();expect(timers.size).toBe(0);
});
it('suspends hidden and context-switched intervals and coalesces focus peer and timer reads',async()=>{
 const f=synchronizationFixture(),pending=deferred<PreferenceRefreshOutcome>();f.read.mockImplementationOnce(()=>pending.promise);f.synchronization.start();expect(f.delay()).toBe(30000);expect(f.read).not.toHaveBeenCalled();const focus=f.synchronization.focus();void f.synchronization.peer();expect(f.read).toHaveBeenCalledTimes(1);f.visible(false);f.synchronization.hidden();expect(f.timers.size).toBe(0);pending.resolve('ok');await focus;expect(f.timers.size).toBe(0);await f.synchronization.peer();expect(f.read).toHaveBeenCalledTimes(1);f.visible(true);await f.synchronization.focus();expect(f.read).toHaveBeenCalledTimes(2);f.synchronization.cancel();expect(f.timers.size).toBe(0);await f.synchronization.peer();expect(f.read).toHaveBeenCalledTimes(2);f.active(false);await f.synchronization.focus();expect(f.read).toHaveBeenCalledTimes(2);f.active(true);await f.synchronization.focus();expect(f.read).toHaveBeenCalledTimes(3);f.synchronization.dispose();
});
it.each(['auth','context'] as const)('stops interval reads after %s until a legitimate focus recovery instead of polling a revoked or replaced account',async result=>{
 const f=synchronizationFixture();f.read.mockResolvedValueOnce(result);f.synchronization.start();await f.fire();expect(f.read).toHaveBeenCalledTimes(1);expect(f.timers.size).toBe(0);await f.synchronization.peer();expect(f.read).toHaveBeenCalledTimes(1);await f.synchronization.focus();expect(f.read).toHaveBeenCalledTimes(2);expect(f.delay()).toBe(30000);f.synchronization.dispose();
});
it('backs failed optional reads off and resets cadence only after a successful current preference read',async()=>{
 const f=synchronizationFixture();f.read.mockResolvedValueOnce('error').mockResolvedValueOnce('error').mockResolvedValueOnce('ok');f.synchronization.start();await f.fire();expect(f.delay()).toBe(60000);await f.synchronization.peer();expect(f.read).toHaveBeenCalledTimes(1);expect(f.delay()).toBe(60000);await f.fire();expect(f.delay()).toBe(120000);await f.fire();expect(f.delay()).toBe(30000);f.synchronization.dispose();
});
it('a disposed host cannot schedule another read after an outstanding response settles',async()=>{
 const f=synchronizationFixture(),pending=deferred<PreferenceRefreshOutcome>();f.read.mockImplementationOnce(()=>pending.promise);f.synchronization.start();const work=f.synchronization.focus();f.synchronization.dispose();pending.resolve('ok');await work;expect(f.timers.size).toBe(0);await f.synchronization.focus();expect(f.read).toHaveBeenCalledTimes(1);
});
it('refreshes deferred server language exactly once after dirty work is clean even when document language already changed',()=>{
 let dirty=true;const refresh=vi.fn(),language=createAccountLanguageRefresh('en',{active:()=>true,dirty:()=>dirty,refresh});language.apply('th');expect(language.pending()).toBe(true);expect(refresh).not.toHaveBeenCalled();dirty=false;language.flush();language.flush();language.apply('th');expect(refresh).toHaveBeenCalledTimes(1);language.observe('en');language.apply('th');expect(refresh).toHaveBeenCalledTimes(1);language.observe('th');language.apply('th');expect(language.pending()).toBe(false);language.flush();expect(refresh).toHaveBeenCalledTimes(1);
});
it('does not refresh server content for matching themes or a fresh server language and preserves a newer intent over a late RSC snapshot',()=>{
 const refresh=vi.fn(),language=createAccountLanguageRefresh('en',{active:()=>true,dirty:()=>false,refresh});language.observe('th');language.apply('th');expect(refresh).not.toHaveBeenCalled();language.apply('en');expect(refresh).toHaveBeenCalledTimes(1);language.observe('th');language.apply('en');expect(refresh).toHaveBeenCalledTimes(1);language.observe('en');language.apply('en');expect(language.pending()).toBe(false);
});
it('cancels a deferred language refresh on context switching and does not resurrect an old scope on a clean event',()=>{
 let active=true;const refresh=vi.fn(),language=createAccountLanguageRefresh('en',{active:()=>active,dirty:()=>true,refresh});language.apply('th');active=false;language.cancel();active=true;language.flush();expect(language.pending()).toBe(false);expect(refresh).not.toHaveBeenCalled();
});
it('preserves a real Front language through an optional seed outage and default version-zero revalidation without creating persisted defaults',async()=>{
 const {client,ports}=setup({key,preference:undefined});client.seedLegacy({language:'th',appearance:'dark'});ports.read.mockResolvedValueOnce(snapshot(0));expect(await client.refresh()).toBe('ok');expect(client.getSnapshot().preference).toEqual(preference(0,'dark','th'));expect(ports.write).not.toHaveBeenCalled();
});
it('reports current read expiry and account replacement without applying another account or hiding the saved preference',async()=>{
 const {client,ports}=setup();ports.read.mockRejectedValueOnce(Object.assign(Error('Expired'),{status:401}));expect(await client.refresh()).toBe('auth');expect(client.getSnapshot().preference).toEqual(preference());ports.read.mockResolvedValueOnce({key:other,preference:preference(99,'dark','th')});expect(await client.refresh()).toBe('context');expect(client.getSnapshot().preference).toEqual(preference());expect(ports.apply).not.toHaveBeenCalled();
});

it('ignores an older same-account periodic snapshot without applying it or stopping current-account revalidation',async()=>{const {client,ports}=setup(snapshot(5,'dark','th'));ports.read.mockResolvedValueOnce(snapshot(4));expect(await client.refresh()).toBe('skipped');expect(client.getSnapshot().preference).toEqual(preference(5,'dark','th'));expect(ports.apply).not.toHaveBeenCalled();ports.read.mockResolvedValueOnce(snapshot(6,'system','en'));expect(await client.refresh()).toBe('ok');expect(client.getSnapshot().preference).toEqual(preference(6,'system','en'));});
it('a hidden or anonymous/context-unavailable surface does not start a periodic read',()=>{const f=synchronizationFixture();f.visible(false);f.synchronization.start();expect(f.timers.size).toBe(0);f.visible(true);f.active(false);f.synchronization.start();expect(f.timers.size).toBe(0);expect(f.read).not.toHaveBeenCalled();f.synchronization.dispose();});
it('keeps a hidden page language refresh pending until it is visible without refreshing business content in the background',()=>{let active=false;const refresh=vi.fn(),language=createAccountLanguageRefresh('en',{active:()=>active,dirty:()=>false,refresh});language.apply('th');language.flush();expect(refresh).not.toHaveBeenCalled();expect(language.pending()).toBe(true);active=true;language.flush();language.flush();expect(refresh).toHaveBeenCalledTimes(1);});

import {createAccountSecurityCommand,accountSecurityAcknowledgement} from '@/lib/identity_access/account-security-client';
import {accountSecurityInput,accountSecuritySeedSchema,type AccountSecuritySeed} from '@/lib/identity_access/account-security';
import {AccountShell} from '@/components/identity_access/account-shell';
import {AccountSessions} from '@/components/identity_access/account-sessions';
import {accountSelfNavigation} from '@/lib/identity_access/account-self-navigation';
import {createElement} from 'react';
import {renderToStaticMarkup} from 'react-dom/server';
vi.mock('next/navigation',()=>({useRouter:()=>({refresh:vi.fn(),push:vi.fn()})}));
it('keeps the exact personal security body and key through lost or malformed acknowledgements and blocks a changed uncertain action',async()=>{
 const calls:{key:string|null;body:BodyInit|null|undefined}[]=[],client=createAccountSecurityCommand(async(_url,init)=>{calls.push({key:new Headers(init?.headers).get('Idempotency-Key'),body:init?.body});if(calls.length===1)throw Error('Lost result');if(calls.length===2)return Response.json({data:{revoked:true}});return Response.json({data:{revoked:true,count:2,reauthenticate:false}});}),input={accountKey:key,expectedRevision:other,scope:'family' as const,family:'c'.repeat(64),reason:'Reviewed original sign-in'};
 await expect(client.revoke(input)).rejects.toThrow('unknown');expect(client.hasPending()).toBe(true);await expect(client.revoke({...input,reason:'Changed review'})).rejects.toThrow('original uncertain');expect(calls).toHaveLength(1);await expect(client.revoke(input)).rejects.toThrow('unknown');expect(client.hasPending()).toBe(true);await expect(client.revoke(input)).resolves.toEqual({revoked:true,count:2,reauthenticate:false});expect(new Set(calls.map(row=>row.key)).size).toBe(1);expect(calls.every(row=>row.body===JSON.stringify(input))).toBe(true);expect(client.hasPending()).toBe(false);
});
it('accepts only minimal parsed personal security outcomes and rejects caller-supplied account or session authority',async()=>{
 for(const value of [null,{},[],{revoked:true,count:-1,reauthenticate:true},{revoked:true,count:1.5,reauthenticate:false},{revoked:true,count:1,reauthenticate:'true'}])expect(accountSecurityAcknowledgement(value)).toBe(false);const input={accountKey:key,expectedRevision:other,scope:'all',reason:'Review all devices'};expect(accountSecurityInput.safeParse({...input,userId:'somebody-else'}).success).toBe(false);expect(accountSecurityInput.safeParse({...input,sessionId:'private-session'}).success).toBe(false);expect(accountSecurityInput.safeParse({...input,family:'c'.repeat(64)}).success).toBe(false);const client=createAccountSecurityCommand(async()=>Response.json({error:{message:'Fresh authenticator verification required'}},{status:403}));await expect(client.revoke(accountSecurityInput.parse(input))).rejects.toThrow('Fresh authenticator');expect(client.hasPending()).toBe(false);
});
it('renders personal Account routes and initial own security content without a mount read or any Company provider or raw-session snapshot',()=>{
 const seed:AccountSecuritySeed={accountKey:key,revision:other,mfaEnabled:true,mfaFreshUntil:null,families:[{handle:'c'.repeat(64),current:true,createdAt:1,expiresAt:2000000000000,userAgent:'Fictional native browser',audiences:['finance','people']}],legacySessions:0},fetcher=vi.spyOn(globalThis,'fetch');
 try{const html=renderToStaticMarkup(createElement(AccountShell,{current:'security'},createElement(AccountSessions,{initial:seed})));for(const href of ['/my-profile','/my-access','/security'])expect(html).toContain(`href="${href}"`);expect(html).toContain('This sign-in');expect(html).toContain('Fictional native browser');expect(html).toContain('Sign out all devices');for(const value of ['permissions_json','authority_json','session_token','company_package_grants','platform_assignments',key,other,'c'.repeat(64)])expect(html).not.toContain(value);expect(fetcher).not.toHaveBeenCalled();expect(accountSecuritySeedSchema.safeParse({...seed,sessionToken:'private'}).success).toBe(false);}finally{fetcher.mockRestore();}
});
it('keeps original Console self links when central Account routing is disabled and separates personal and Company profiles when enabled',()=>{
 expect(accountSelfNavigation('/holdings/acme','own-user')).toEqual({access:'/holdings/acme/iam/my-access',profile:'/holdings/acme/profiles/own-user',companyProfile:null});expect(accountSelfNavigation('/platform','own-user')).toEqual({access:'/platform/iam/my-access',profile:null,companyProfile:null});expect(accountSelfNavigation('/holdings/acme/branches/b1','own/user',true)).toEqual({access:'/my-access',profile:'/my-profile',companyProfile:'/holdings/acme/branches/b1/profiles/own%2Fuser'});expect(accountSelfNavigation('/platform','own-user',true)).toEqual({access:'/my-access',profile:'/my-profile',companyProfile:null});
});

import {readFileSync} from 'node:fs';
import ts from 'typescript';
import type {AccountProfile} from '@/server/modules/identity_access/public/account-preferences';
import {accountProfileAcknowledgement,accountProfilePreferenceEvent,accountProfileResponseCurrent,observeAccountProfileWitness,type AccountProfileWitness} from '@/lib/identity_access/account-profile-client';
import {AccountProfileForm} from '@/components/identity_access/account-profile-form';

/** Execute the original owner handler with explicit state/browser ports. This
 * does not simulate React hooks or claim a browser/hydration observation. */
function accountOwnerDeclaration(source:string,name:string,kind:'function'|'variable'){
 const tree=ts.createSourceFile('account-owner.tsx',source,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);let found:ts.Node|undefined;
 function visit(node:ts.Node):void{if(found)return;if(kind==='function'&&ts.isFunctionDeclaration(node)&&node.name?.text===name)found=node;else if(kind==='variable'&&ts.isVariableDeclaration(node)&&ts.isIdentifier(node.name)&&node.name.text===name)found=node;ts.forEachChild(node,visit);}
 visit(tree);if(!found)throw Error(`Missing actual Account owner declaration ${name}`);return(kind==='variable'?'const ':'')+found.getText(tree)+(kind==='variable'?';':'');
}
function accountOwnerCallable(source:string,name:string,kind:'function'|'variable',ports:Record<string,unknown>,prefix=''){
 const js=ts.transpileModule(prefix+'\n'+accountOwnerDeclaration(source,name,kind)+'\nreturn '+name+';',{
  compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None},
 }).outputText;
 const result:unknown=Function(...Object.keys(ports),js)(...Object.values(ports));if(typeof result!=='function')throw Error('Actual Account handler was not callable');return result;
}
const profileOwnerSource=()=>readFileSync(new URL('../../../src/components/identity_access/account-profile-form.tsx',import.meta.url),'utf8');
const preferenceProviderSource=()=>readFileSync(new URL('../../../src/components/identity_access/account-preferences-provider.tsx',import.meta.url),'utf8');

type AccountProfileFormState={received:AccountProfile;baseline:AccountProfile;draft:Pick<AccountProfile,'name'>&Pick<AccountPreferences,'language'|'appearance'>};
function actualAccountProfileFixture(options:{accountKey?:string;initialVersion?:number;priorWitness?:AccountProfileWitness;clean?:boolean;preferenceRead?:AccountPreferences;holdPreferenceWrite?:boolean}={}){
 const profileSource=profileOwnerSource(),providerSource=preferenceProviderSource(),accountKey=options.accountKey??key;
 const initial:AccountProfile={name:'Original own name',email:'own@example.test',preferences:preference(options.initialVersion??1,'system','en')};
 let state:AccountProfileFormState={received:initial,baseline:initial,draft:{name:'Captured own name',language:'en',appearance:'light'}},saved=false,invalidated=false;
 if(options.clean)state={...state,draft:{name:initial.name,language:initial.preferences.language,appearance:initial.preferences.appearance}};
 const browser=Object.assign(new EventTarget(),{confirm:vi.fn(()=>true)}),cookies:string[]=[],theme=vi.fn((_name:string,_value:string)=>{void _name;void _value;});
 const documentPort={documentElement:{lang:'en',setAttribute:theme},visibilityState:'visible',cookie:''};
 Object.defineProperty(documentPort,'cookie',{get:()=>cookies.at(-1)??'',set:(value:string)=>cookies.push(value)});
 let body!:ReadableStreamDefaultController<Uint8Array>;
 const held=new Response(new ReadableStream<Uint8Array>({start(controller){body=controller;}}),{headers:{'Content-Type':'application/json'}});
 let preferenceBody:ReadableStreamDefaultController<Uint8Array>|undefined;
 const heldPreference=options.holdPreferenceWrite?new Response(new ReadableStream<Uint8Array>({start(controller){preferenceBody=controller;}}),{headers:{'Content-Type':'application/json'}}):undefined;
 const transport=vi.fn(async(url:string,init?:RequestInit)=>url.startsWith('/api/v1/account/profile')?held:init?.method==='PATCH'&&heldPreference?heldPreference:Response.json({data:{key:accountKey,preference:options.preferenceRead??preference(3,'dark','en')}}));
 const browserPorts={document:documentPort,window:browser,location:{protocol:'https:'},CustomEvent,fetch:transport,
  contextGeneration:()=>0,contextIsSwitching:()=>false,hasUnsavedWork:()=>true,BroadcastChannel:undefined,
  createAccountPreferencesClient,createAccountLanguageRefresh};
 const provider=accountOwnerCallable(providerSource,'model','function',browserPorts,
  accountOwnerDeclaration(providerSource,'writeCookie','function')+'\n'+accountOwnerDeclaration(providerSource,'responseSnapshot','function'))({key:accountKey,preference:initial.preferences},vi.fn());
 const savedListener=accountOwnerCallable(providerSource,'saved','variable',{holder:provider,client:provider.client});
 browser.addEventListener('erpx:account-preferences-saved',event=>savedListener(event));
 const controller={current:null as AbortController|null},epoch={current:0},refresh=vi.fn(),errors=vi.fn();
 const witness:{current:AccountProfileWitness}={current:observeAccountProfileWitness(options.priorWitness??{accountKey,version:initial.preferences.version,foreignAccount:false,pending:false},accountKey,initial.preferences.version,provider.client.getSnapshot())},edits={current:0};
 // Explicit state-observation port, using the same production observer as the
 // form's committed context/seed observations, without implementing hooks.
 provider.client.subscribe(()=>{witness.current=observeAccountProfileWitness(witness.current,accountKey,initial.preferences.version,provider.client.getSnapshot());});
 const ports=()=>({...browserPorts,state,accountKey,th:false,hydrated:true,invalidated,controller,epoch,
  sharedPreferences:provider.client.getSnapshot(),witness,edits,accountProfileAcknowledgement,accountProfilePreferenceEvent,accountProfileResponseCurrent,observeAccountProfileWitness,
  setPending:vi.fn(),setError:errors,setSaved:(next:boolean)=>{saved=next;},setInvalidated:(next:boolean)=>{invalidated=next;},
  setState:(update:AccountProfileFormState|((current:AccountProfileFormState)=>AccountProfileFormState))=>{state=typeof update==='function'?update(state):update;},router:{refresh}});
 const prefix=accountOwnerDeclaration(profileSource,'draftOf','variable')+'\n'+accountOwnerDeclaration(profileSource,'unchanged','variable');
 const ownObserved=accountOwnerCallable(profileSource,'observed','variable',ports());
 browser.addEventListener('erpx:account-preferences-saved',event=>ownObserved(event));
 return{initial,accountKey,provider,transport,theme,cookies,errors,witness,refresh,state:()=>state,saved:()=>saved,
  submit:()=>accountOwnerCallable(profileSource,'submit','function',ports(),prefix)({preventDefault:vi.fn(),currentTarget:{reportValidity:()=>true}}),
  load:()=>accountOwnerCallable(profileSource,'loadSaved','function',{...ports(),dirty:true},prefix)(),
  edit:(name:string)=>accountOwnerCallable(profileSource,'edit','function',ports())({name}),
  serverSeed:(next:AccountProfile)=>{
   const tree=ts.createSourceFile('actual-profile.tsx',profileSource,ts.ScriptTarget.Latest,true,ts.ScriptKind.TSX);let statement:ts.IfStatement|undefined;
   function visit(node:ts.Node):void{if(ts.isIfStatement(node)&&ts.isBinaryExpression(node.expression)&&ts.isPropertyAccessExpression(node.expression.left)&&node.expression.left.expression.getText(tree)==='state'&&node.expression.left.name.text==='received')statement=node;ts.forEachChild(node,visit);}visit(tree);
   if(!statement)throw Error('Missing actual Profile server-seed transition');
   const bindings={...ports(),initial:next},js=ts.transpileModule(prefix+'\n'+statement.getText(tree),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.None}}).outputText;
   Function(...Object.keys(bindings),js)(...Object.values(bindings));
   witness.current=observeAccountProfileWitness(witness.current,accountKey,next.preferences.version,provider.client.getSnapshot());provider.client.observe({key:accountKey,preference:next.preferences});
  },
  invalidate:()=>accountOwnerCallable(profileSource,'changed','variable',ports())(),
  preferenceEvent:(eventKey:string,version:number)=>browser.dispatchEvent(new CustomEvent('erpx:account-preferences-saved',{detail:{key:eventKey,preference:preference(version,'dark','en')}})),
  release:(value:unknown)=>{body.enqueue(new TextEncoder().encode(JSON.stringify(value)));body.close();},
  releasePreference:(value:unknown)=>{if(!preferenceBody)throw Error('No held native preference response');preferenceBody.enqueue(new TextEncoder().encode(JSON.stringify(value)));preferenceBody.close();},
  native:(version=initial.preferences.version+1)=>({data:{...initial,name:'Captured own name',preferences:preference(version,'light','en')}}),
 };
}

it('the actual Account Profile PATCH handler cannot replace newer same-account preferences after a delayed native JSON body',async()=>{
 const f=actualAccountProfileFixture(),saving=f.submit();await settled();
 expect(f.transport).toHaveBeenCalledTimes(1);expect(f.transport.mock.calls[0][1]).toMatchObject({method:'PATCH',body:JSON.stringify({...f.state().draft,accountKey:key,expectedName:f.initial.name,expectedVersion:1})});
 await f.provider.client.refresh();expect(f.provider.client.getSnapshot().preference).toEqual(preference(3,'dark','en'));
 expect(f.theme).toHaveBeenLastCalledWith('data-theme','dark');f.release(f.native());await saving;
 expect(f.provider.client.getSnapshot().preference).toEqual(preference(3,'dark','en'));
 expect(f.theme).toHaveBeenLastCalledWith('data-theme','dark');
 expect(f.cookies.filter(value=>value.startsWith('erpx_appearance=')).at(-1)).toContain('erpx_appearance=dark;');
 expect(f.saved()).toBe(false);expect(f.state().draft.name).toBe('Captured own name');
});
it('keeps later Profile edits and the exact original CAS body when a current parsed acknowledgement arrives',async()=>{
 const f=actualAccountProfileFixture(),saving=f.submit();await settled();f.edit('Later own draft');f.release(f.native());await saving;
 expect(f.state().baseline).toMatchObject({name:'Captured own name',preferences:{version:2}});expect(f.state().draft.name).toBe('Later own draft');expect(f.saved()).toBe(false);
 expect(JSON.parse(String(f.transport.mock.calls[0][1]?.body))).toMatchObject({name:'Captured own name',expectedName:'Original own name',expectedVersion:1});expect(f.transport).toHaveBeenCalledTimes(1);
});
it.each([null,{}, {data:{}},{data:{name:'Captured own name',email:'own@example.test',preferences:{language:{value:'en'},appearance:'light',version:2}}},{data:{name:'Captured own name',email:'own@example.test',preferences:{language:'en',appearance:'wrong',version:2}}},{data:{name:'Captured own name',email:'own@example.test',preferences:{language:'en',appearance:'light',version:-1}}},{data:{name:'Captured own name',email:'own@example.test',preferences:{language:'en',appearance:'light',version:1.5}}}])('rejects malformed successful Profile body %# before acknowledging or painting it',async body=>{
 const f=actualAccountProfileFixture(),saving=f.submit();await settled();f.release(body);await saving;
 expect(f.saved()).toBe(false);expect(f.state().baseline).toEqual(f.initial);expect(f.state().draft.name).toBe('Captured own name');expect(f.theme).not.toHaveBeenCalled();expect(f.cookies).toEqual([]);expect(f.errors).toHaveBeenLastCalledWith(expect.stringContaining('saved account settings'));
});
it('rejects a late Load saved settings projection after a newer same-account owner read without clearing the original draft',async()=>{
 const f=actualAccountProfileFixture(),loading=f.load();await settled();await f.provider.client.refresh();f.release(f.native());await loading;
 expect(f.transport.mock.calls[0]).toMatchObject([`/api/v1/account/profile?accountKey=${key}`,{cache:'no-store'}]);expect(f.state().baseline).toEqual(f.initial);expect(f.state().draft.name).toBe('Captured own name');expect(f.theme).toHaveBeenLastCalledWith('data-theme','dark');expect(f.refresh).not.toHaveBeenCalled();
});
it('accepts the current confirmed Load saved settings result and preserves an edit made after that confirmation',async()=>{
 const f=actualAccountProfileFixture(),loading=f.load();await settled();f.edit('Later after confirmation');f.release(f.native(3));await loading;
 expect(f.state().baseline.preferences.version).toBe(3);expect(f.state().draft.name).toBe('Later after confirmation');expect(f.refresh).toHaveBeenCalledTimes(1);expect(f.transport).toHaveBeenCalledTimes(1);
});
it('uses only same-account owner preference events and accepts a different account original lower version',async()=>{
 const f=actualAccountProfileFixture({accountKey:other,initialVersion:0,priorWitness:{accountKey:key,version:90,foreignAccount:false,pending:true}}),saving=f.submit();await settled();f.preferenceEvent(key,99);expect(f.witness.current.version).toBe(0);f.release(f.native());await saving;
 expect(f.saved()).toBe(true);expect(f.state().baseline.preferences.version).toBe(1);expect(f.provider.client.getSnapshot().preference.version).toBe(1);expect(JSON.parse(String(f.transport.mock.calls[0][1]?.body))).toMatchObject({accountKey:other,expectedVersion:0});
});
it('invalidates the original Profile handler after context cancellation even when transport ignores abort during JSON',async()=>{
 const f=actualAccountProfileFixture(),saving=f.submit();await settled();f.invalidate();f.release(f.native());await saving;
 expect(f.saved()).toBe(false);expect(f.state().baseline).toEqual(f.initial);expect(f.theme).not.toHaveBeenCalled();expect(f.cookies).toEqual([]);expect(f.refresh).not.toHaveBeenCalled();
});
it('uses a newer actual Profile server seed without losing dirty values or acknowledging the old body over it',async()=>{
 const f=actualAccountProfileFixture(),saving=f.submit();await settled();const server={...f.initial,name:'New server own name',preferences:preference(3,'dark','en')};f.serverSeed(server);
 expect(f.state().received).toEqual(server);expect(f.state().baseline).toEqual(f.initial);expect(f.state().draft.name).toBe('Captured own name');f.release(f.native());await saving;
 expect(f.state().baseline).toEqual(f.initial);expect(f.state().draft.name).toBe('Captured own name');expect(f.saved()).toBe(false);expect(f.theme).toHaveBeenLastCalledWith('data-theme','dark');
});
it('accepts a current clean server seed and never regresses it on an older refresh or causes a mount GET',()=>{
 const f=actualAccountProfileFixture({clean:true}),next={...f.initial,name:'Current server name',preferences:preference(3,'dark','th')};f.serverSeed(next);expect(f.state().baseline).toEqual(next);expect(f.state().draft).toEqual({name:next.name,language:'th',appearance:'dark'});
 f.serverSeed({...f.initial,preferences:preference(2,'light','en')});expect(f.state().baseline).toEqual(next);expect(f.state().draft.name).toBe(next.name);expect(f.transport).not.toHaveBeenCalled();
});
it('observes the actual same-account preference event before accepting a delayed Profile body',async()=>{
 const f=actualAccountProfileFixture(),saving=f.submit();await settled();f.preferenceEvent(key,3);expect(f.witness.current.version).toBe(3);f.release(f.native());await saving;
 expect(f.saved()).toBe(false);expect(f.state().baseline).toEqual(f.initial);expect(f.theme).toHaveBeenLastCalledWith('data-theme','dark');expect(f.transport).toHaveBeenCalledTimes(1);
});
it('renders the actual personal Profile server content without an initial GET or raw account/context witness in the HTML',()=>{
 const fetcher=vi.spyOn(globalThis,'fetch'),initial:AccountProfile={name:'Own visible profile',email:'own@example.test',preferences:preference(0,'dark','th')};
 try{const html=renderToStaticMarkup(createElement(AccountProfileForm,{accountKey:key,initial,language:'th'}));expect(html).toContain('Own visible profile');expect(html).toContain('own@example.test');expect(html).toContain('dark');expect(html).not.toContain(key);expect(fetcher).not.toHaveBeenCalled();}finally{fetcher.mockRestore();}
});
it('the actual Account Profile acknowledgement cannot cancel a newer pending original provider preference intent at the same saved version',async()=>{
 const f=actualAccountProfileFixture({preferenceRead:preference(2,'light','en'),holdPreferenceWrite:true}),saving=f.submit();await settled();
 await f.provider.client.refresh();f.provider.client.change({appearance:'dark'});await settled();
 const request=f.transport.mock.calls.find(call=>call[0]==='/api/v1/account/preferences'&&call[1]?.method==='PATCH');
 expect(request?.[1]?.body).toBe(JSON.stringify({accountKey:key,expectedVersion:2,language:'en',appearance:'dark'}));
 expect(f.provider.client.getSnapshot()).toMatchObject({pending:true,preference:preference(2,'dark','en')});
 f.release(f.native());await saving;
 const retainedPending=f.provider.client.getSnapshot().pending,aborted=request?.[1]?.signal?.aborted;
 f.releasePreference({data:{key,preference:preference(3,'dark','en')}});await settled();
 expect(retainedPending).toBe(true);expect(aborted).toBe(false);
 expect(f.provider.client.getSnapshot()).toMatchObject({pending:false,preference:preference(3,'dark','en')});
 expect(f.theme).toHaveBeenLastCalledWith('data-theme','dark');expect(f.cookies.filter(value=>value.startsWith('erpx_appearance=')).at(-1)).toContain('erpx_appearance=dark;');
 expect(f.saved()).toBe(false);expect(f.state().baseline).toEqual(f.initial);expect(f.state().draft.name).toBe('Captured own name');expect(f.refresh).not.toHaveBeenCalled();
});
