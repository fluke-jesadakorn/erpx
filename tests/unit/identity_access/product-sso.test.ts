import { expect, it } from "vitest";
import { anonymousAccountAuthPath, bindingIsCurrent, safeProductReturnPath, sessionLifetime } from "@erpx/server-identity_access/domain/product-sso";
import { productWorkspaceTarget } from "@erpx/server-identity_access/domain/product-workspace";
it("restricts product Company selection to its own Office pages and preserves destination queries", () => {
  const next = "/holdings/acme/branches/b1/departments/d1/finance/documents?status=paid&filter=a%2Fb#selected";
  expect(productWorkspaceTarget("finance", "company-two", next)).toEqual({ companyId: "company-two", target: next,
    scope: { kind: "department", holdingSlug: "acme", branchSlug: "b1", departmentSlug: "d1" } });
  for (const target of ["//evil.test", "/api/auth/sign-out", "/holdings/acme/people-organization/people/directory/current", "/holdings/%zz/finance/documents", "/holdings/acme%2Fother/finance/documents", "/launch", "/sso/callback"]) expect(productWorkspaceTarget("finance", "c1", target), target).toBeNull();
  for (const audience of ["account", "apps", "booking", "portal", "legal-portal", "forms", "pos", "pms", "admin"] as const) expect(productWorkspaceTarget(audience, "c1", "/holdings/acme/finance/documents")).toBeNull();
  expect(productWorkspaceTarget("finance", null, next)).toBeNull();
  expect(safeProductReturnPath("/launch?companyId=c2&next=%2Fholdings%2Facme%2Ffinance%2Fdocuments")).toContain("companyId=c2");
});
it("requires bound identity for account mutation and device-session endpoints", () => {
  for (const path of ["/api/auth/update-user", "/api/auth/change-password", "/api/auth/multi-session/set-active", "/api/auth/list-sessions"]) expect(anonymousAccountAuthPath(path)).toBe(false);
  for (const path of ["/api/auth/sign-in/email", "/api/auth/directory/sign-in", "/api/auth/request-password-reset", "/api/auth/reset-password/original_token-123"]) expect(anonymousAccountAuthPath(path)).toBe(true);
  for (const path of ["/api/auth/reset-password/token/admin", "/api/auth/reset-password/%2fadmin", "/api/auth/reset-password/..", "/api/auth/reset-password/%0a"]) expect(anonymousAccountAuthPath(path)).toBe(false);
});
it("keeps SSO returns same-origin and removes callback credentials", () => {
  for (const value of ["//evil.test", "https://evil.test", "/\\evil.test", "/%2f/evil.test", "/%5cevil.test", "/sso/callback?code=x", "/api/auth/sign-out", "/bad%0aHeader", "/bad%zz", "/login"]) expect(safeProductReturnPath(value), value).toBe("/");
  expect(safeProductReturnPath("/holdings/acme/finance/documents?status=paid&code=secret&state=stale#token")).toBe("/holdings/acme/finance/documents?status=paid");
});
it("binds audience and environment to original authentication time with an absolute 12-hour cap", () => {
  const binding = { sessionId: "s1", audience: "finance", environment: "development", parentSessionId: "p1", authenticatedAt: 1000, absoluteExpiresAt: 1000 + sessionLifetime };
  expect(bindingIsCurrent(binding, "finance", "development", 1001)).toBe(true);
  expect(bindingIsCurrent(binding, "people", "development", 1001)).toBe(false);
  expect(bindingIsCurrent(binding, "finance", "production", 1001)).toBe(false);
  expect(bindingIsCurrent(binding, "finance", "development", 1000 + sessionLifetime)).toBe(false);
  expect(bindingIsCurrent({ ...binding, absoluteExpiresAt: 1001 + sessionLifetime }, "finance", "development", 1001)).toBe(false);
});

import {createProviderContextClient} from '@erpx/feature-identity_access/lib/provider-context-client';
import {providerContextSelection,providerContextAcknowledgement,type ProviderContextSeed} from '@erpx/feature-identity_access/lib/provider-context';
const providerSeed:ProviderContextSeed={accountKey:'a'.repeat(64),contextKey:'b'.repeat(64),revision:0,selectedId:'assigned-a',choices:[{id:'assigned-a',label:'Assigned administrator',version:'c'.repeat(64)},{id:'assigned-b',label:'Assigned auditor',version:'d'.repeat(64)}]};
const providerAfter:ProviderContextSeed={...providerSeed,contextKey:'e'.repeat(64),selectedId:'assigned-b',revision:1};
it('provider context starts from the exact SSR seed without a mount read or any private authority fields',()=>{
 let reads=0,writes=0;const client=createProviderContextClient(providerSeed,{read:async()=>{reads++;return providerSeed;},write:async()=>{writes++;return providerAfter;},key:()=> 'captured-key',generation:()=>0});expect(client.getSnapshot().seed).toEqual(providerSeed);client.observe(providerSeed);expect([reads,writes]).toEqual([0,0]);client.dispose();
});
it('provider context retains the same captured body and key after lost or malformed acknowledgement',async()=>{
 const bodies:unknown[]=[],keys:string[]=[];let attempt=0;const client=createProviderContextClient(providerSeed,{read:async()=>providerSeed,write:async(input,key)=>{bodies.push(input);keys.push(key);if(attempt++===0)throw Error('Lost response');return attempt===2?{...providerAfter,selectedId:'assigned-a'}:providerAfter;},key:()=> 'exact-captured-key',generation:()=>0});await expect(client.select('assigned-b')).rejects.toThrow('Lost response');expect(client.getSnapshot().retry).toBe(true);await expect(client.select('assigned-a')).rejects.toThrow('Retry or review');await expect(client.retry()).rejects.toThrow('confirmation did not match');await client.retry();expect(keys).toEqual(['exact-captured-key','exact-captured-key','exact-captured-key']);expect(bodies).toEqual([providerContextSelection(providerSeed,'assigned-b'),providerContextSelection(providerSeed,'assigned-b'),providerContextSelection(providerSeed,'assigned-b')]);expect(client.getSnapshot()).toMatchObject({seed:providerAfter,pending:false,retry:false});client.dispose();
});
it('provider context rejects foreign accounts, wrong role versions and wrong captured revisions before acknowledgement',()=>{
 const input=providerContextSelection(providerSeed,'assigned-b');for(const result of [{...providerAfter,accountKey:'f'.repeat(64)},{...providerAfter,revision:2},{...providerAfter,choices:providerAfter.choices.map(choice=>({...choice,version:'f'.repeat(64)}))}])expect(()=>providerContextAcknowledgement(result,input)).toThrow();
});
it('provider context discards an old focus response after a newer SSR context and stops reading on dispose',async()=>{
 let finish:(value:unknown)=>void=()=>{},signal:AbortSignal|undefined;const client=createProviderContextClient(providerSeed,{read:received=>{signal=received;return new Promise(resolve=>{finish=resolve;});},write:async()=>providerAfter,key:()=> 'key',generation:()=>0}),reading=client.refresh();client.observe(providerAfter);expect(signal?.aborted).toBe(true);finish(providerSeed);await reading;expect(client.getSnapshot().seed).toEqual(providerAfter);client.dispose();await client.refresh();expect(client.getSnapshot().seed).toEqual(providerAfter);
});
it('provider context does not accept a late selection after the account context generation changes',async()=>{
 let generation=0,finish:(value:unknown)=>void=()=>{};const client=createProviderContextClient(providerSeed,{read:async()=>providerSeed,write:()=>new Promise(resolve=>{finish=resolve;}),key:()=> 'key',generation:()=>generation}),selection=client.select('assigned-b');generation++;finish(providerAfter);await expect(selection).rejects.toThrow('context changed');expect(client.getSnapshot().seed).toEqual(providerSeed);client.dispose();
});
it('provider focus and peer refresh are read-only and require review before replacing another tab context',async()=>{
 let writes=0;const client=createProviderContextClient(providerSeed,{read:async()=>providerAfter,write:async()=>{writes++;return providerAfter;},key:()=> 'key',generation:()=>0});await client.refresh();expect(client.getSnapshot()).toMatchObject({seed:providerAfter,stale:true});expect(writes).toBe(0);client.observe(providerAfter);expect(client.getSnapshot().stale).toBe(false);client.dispose();
});
it('provider Admin switcher renders its actual minimal SSR role seed without a mount fetch',async()=>{
 const {createElement}=await import('react'),{renderToStaticMarkup}=await import('react-dom/server'),{ProviderContextBoundary}=await import('@erpx/feature-identity_access/components/provider-context-boundary'),{ProviderContextSwitcher}=await import('@erpx/feature-identity_access/components/provider-context-switcher'),{ContextSwitchContext}=await import('@erpx/workplace/components/context-switch-context');
 const html=renderToStaticMarkup(createElement(ContextSwitchContext.Provider,{value:{busy:false,switchContext:async()=>true}},createElement(ProviderContextBoundary,{initial:providerSeed,children:createElement(ProviderContextSwitcher)})));expect(html).toContain('Provider role');expect(html).toContain('Assigned administrator');expect(html).toContain('Assigned auditor');expect(html).not.toContain('modules_json');expect(html).not.toContain('sessionId');
});
it('provider known stale, wrong-account and unresolved selections hide dependent tools while retaining the mounted guarded draft',async()=>{
 const {providerContextBlocked}=await import('@erpx/feature-identity_access/lib/provider-context-client');expect(providerContextBlocked({stale:false,retry:false,pending:false})).toBe(false);for(const flags of [{stale:true,retry:false,pending:false},{stale:false,retry:true,pending:false},{stale:false,retry:false,pending:true}])expect(providerContextBlocked(flags)).toBe(true);
});
it('provider context survives StrictMode setup cleanup reacquisition without a mount read or disabling later role selection',async()=>{
 let reads=0,writes=0;const client=createProviderContextClient(providerSeed,{read:async()=>{reads++;return providerSeed;},write:async()=>{writes++;return providerAfter;},key:()=> 'strict-role-selection',generation:()=>0});client.activate();client.dispose();client.activate();client.observe(providerSeed);expect([reads,writes]).toEqual([0,0]);await client.select('assigned-b');expect(client.getSnapshot()).toMatchObject({seed:providerAfter,pending:false,retry:false});expect(writes).toBe(1);client.dispose();
});
it('provider reacquisition rejects an old aborted acknowledgement and retains the exact uncertain selection for explicit retry',async()=>{
 const keys:string[]=[],bodies:unknown[]=[];let resolve:(value:unknown)=>void=()=>{},attempt=0;const client=createProviderContextClient(providerSeed,{read:async()=>providerSeed,write:async(input,key)=>{keys.push(key);bodies.push(input);if(attempt++===0)return new Promise(done=>{resolve=done;});return providerAfter;},key:()=> 'strict-exact-key',generation:()=>0}),pending=client.select('assigned-b');client.dispose();client.activate();resolve(providerAfter);await expect(pending).rejects.toThrow('context changed');expect(client.getSnapshot()).toMatchObject({seed:providerSeed,pending:false,retry:true});await client.retry();expect(client.getSnapshot().seed).toEqual(providerAfter);expect(keys).toEqual(['strict-exact-key','strict-exact-key']);expect(bodies).toEqual([providerContextSelection(providerSeed,'assigned-b'),providerContextSelection(providerSeed,'assigned-b')]);client.dispose();
});
