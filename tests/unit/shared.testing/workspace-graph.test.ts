import{workspaceBrowserFixtureProfile,workspaceHoldingProfile,workspaceBrowserOrigins,assertWorkspaceHoldingBinding}from '../../../scripts/testing/development-host-profile';
import{firstRouteCompile,firstRouteEnsure}from '../../../scripts/performance/route-compile-metrics';
import{runtimeOwnerCycles}from '../../../scripts/architecture/owner-cycles';
import { mkdtemp, mkdir, writeFile, rm, realpath } from 'node:fs/promises';
import {readFileSync} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { runtimeSourceFingerprint } from '../../../scripts/local/source-fingerprint';
import { describe, expect, it, vi } from 'vitest';
import { createWorkspaceGraph, selectWorkspaceSuites, selectCandidateSuites, appRuntimeFingerprint, workspaceFingerprint } from '../../../scripts/testing/workspace-graph';
import type { Suite } from '../../../scripts/testing/catalogue';
import { workspaceStyleSheet } from '../../../scripts/assets/workspace-styles';
import { Scanner } from '@tailwindcss/oxide';
import {watchWorkspaceStyleSources,workspaceStyleGraphChanged,workspaceStyleWatchInput} from '../../../scripts/local/workspace-style-watcher';
const suite=(id:string,app:string,file:string):Suite=>({id,lane:'unit',owner:'shared.testing',modules:['finance'],files:[file],fixture:'none',requires:{jobs:false,scanner:false},apps:[app],packages:['@erpx/contracts']});
function sources(){return new Map([
 ['apps/finance/src/app/page.tsx','import "@erpx/contracts/finance";'],
 ['apps/people/src/app/page.tsx','import "@erpx/contracts/people";'],
 ['packages/contracts/src/finance.ts','import { currency } from "./money"; import type { Person } from "./person"; export {currency};'],
 ['packages/contracts/src/people.ts','export const employee=1;'],
 ['packages/contracts/src/person.ts','export type Person={name:string};'],
 ['packages/contracts/src/money.ts','export const currency="THB";'],
 ['tests/unit/shared.testing/finance.test.ts','import "@erpx/contracts/finance";'],
 ['tests/unit/shared.testing/people.test.ts','import "@erpx/contracts/people";'],
 ['public/brand/icon.svg','asset-sha256:one'],
 ['bun.lock','locked'],['tsconfig.base.json','{}']]);}
const suites=[suite('finance','finance','tests/unit/shared.testing/finance.test.ts'),suite('people','people','tests/unit/shared.testing/people.test.ts')];
it('keeps CSS invalidation within actual clients even when another app shares their package owner',()=>{
 const files=sources();
 files.set('apps/finance/src/app/page.tsx','import "./globals.css"; import "@erpx/feature-example/finance";');
 files.set('apps/people/src/app/page.tsx','import "./globals.css"; import "@erpx/feature-example/people";');
 files.set('packages/feature-example/src/finance.tsx','import type {Data} from "./types"; import("./lazy"); export const Panel=()=> <div className="p-4"/>;');
 files.set('packages/feature-example/src/people.tsx','export const Panel=()=> <div className="p-5"/>;');
 files.set('packages/feature-example/src/types.ts','export type Data={label:string};');
 files.set('packages/feature-example/src/lazy.tsx','export const Lazy=()=> <div className="p-6"/>;');
 files.set('packages/workplace/src/routes/globals.css','');
 // Empty styles allow deriving the initial executable graph without any broad scan.
 for(const id of ['finance','people'])files.set(`apps/${id}/src/app/globals.css`,'');
 const executable=createWorkspaceGraph(files);
 for(const id of ['finance','people'])files.set(`apps/${id}/src/app/globals.css`,workspaceStyleSheet(executable,`apps/${id}`));
 const css=files.get('apps/finance/src/app/globals.css')!;
 expect(css).toContain('feature-example/src/finance.tsx');expect(css).toContain('feature-example/src/lazy.tsx');
 expect(css).not.toContain('feature-example/src/people.tsx');expect(css).not.toContain('feature-example/src/types.ts');
 const before=createWorkspaceGraph(files),changed=new Map(files);
 changed.set('packages/feature-example/src/people.tsx','export const Panel=()=> <div className="p-8"/>;');
 const after=createWorkspaceGraph(changed);
 expect(appRuntimeFingerprint(after,'finance')).toBe(appRuntimeFingerprint(before,'finance'));
 expect(appRuntimeFingerprint(after,'people')).not.toBe(appRuntimeFingerprint(before,'people'));
 expect(selectCandidateSuites(after,before,suites,['packages/feature-example/src/people.tsx']).apps).toEqual(['people']);
});

it('scans an exact Next route filename with literal brackets using the installed Tailwind scanner',async()=>{
 const root=await mkdtemp(join(tmpdir(),'erpx-style-source-'));
 try{
  const file=join(root,'routes/[holdingSlug]/page.tsx');await mkdir(dirname(file),{recursive:true});
  await writeFile(file,'<div className="bg-red-500 p-7"/>');
  await writeFile(join(root,'routes/unrelated.tsx'),'<div className="bg-blue-500"/>');
  const scanner=new Scanner({sources:[{base:root,pattern:'routes/[holdingSlug]/page.tsx',negated:false}]}),candidates=scanner.scan();
  expect(candidates).toContain('bg-red-500');expect(candidates).not.toContain('bg-blue-500');expect(scanner.files).toEqual([await realpath(file)]);
 }finally{await rm(root,{recursive:true,force:true});}
});

it('refreshes style membership for executable edges, additions and deletion while ignoring ordinary HMR edits',()=>{
 const file='packages/ui/src/panel.tsx';
 expect(workspaceStyleGraphChanged(file,'import "./button"; const label="Old";','import "./button"; const label="New";')).toBe(false);
 expect(workspaceStyleGraphChanged(file,'import type {Data} from "./old";','import type {Data} from "./new";')).toBe(false);
 expect(workspaceStyleGraphChanged(file,'import "./old";','import "./new";')).toBe(true);
 expect(workspaceStyleGraphChanged(file,'const load=()=>null;','const load=()=>import("./new");')).toBe(true);
 expect(workspaceStyleGraphChanged(file,undefined,'export const label="New";')).toBe(true);
 expect(workspaceStyleGraphChanged(file,'export const label="Old";',undefined)).toBe(true);
 expect(workspaceStyleGraphChanged('packages/ui/package.json','{"exports":{}}','{"exports":{"./new":"./src/new.tsx"}}')).toBe(true);
 for(const path of ['apps/account/.next/types/routes.ts','packages/ui/node_modules/example/src/ui.tsx','apps/account/src/app/globals.css'])expect(workspaceStyleWatchInput(path)).toBe(false);
});

it('regenerates style inputs after a rename without weakening normal missing-source validation',()=>{
 const files=sources();
 files.set('apps/finance/src/app/page.tsx','import "./globals.css"; import "@erpx/feature-example/renamed";');
 files.set('apps/finance/src/app/globals.css','@source "../../../../packages/feature-example/src/removed.tsx";');
 files.set('packages/feature-example/src/renamed.tsx','export const Panel=()=> <div className="p-7"/>;');
 files.set('packages/workplace/src/routes/globals.css','');
 expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_STYLE_SOURCE_UNRESOLVED');
 const derivation=createWorkspaceGraph(files,undefined,{excludeStyleSources:true}),css=workspaceStyleSheet(derivation,'apps/finance');
 expect(css).toContain('feature-example/src/renamed.tsx');expect(css).not.toContain('removed.tsx');
 files.set('apps/finance/src/app/globals.css',css);
 expect(()=>createWorkspaceGraph(files)).not.toThrow();
});

it('updates newly imported sources during a live watch and closes its owned watchers',async()=>{
 const root=await mkdtemp(join(tmpdir(),'erpx-style-watch-')),file='apps/account/src/panel.tsx',initial='export const Panel=()=> <div className="p-4"/>;',report=vi.fn();
 const inputs=sources();inputs.set(file,initial);inputs.set('packages/ui/src/new.tsx','export const New=()=> <div className="p-7"/>;');
 let css=workspaceStyleSheet(createWorkspaceGraph(inputs),'apps/account');
 const refresh=vi.fn(()=>{const current=new Map(inputs);current.set(file,readFileSync(join(root,file),'utf8'));css=workspaceStyleSheet(createWorkspaceGraph(current),'apps/account');});
 expect(css).not.toContain('packages/ui/src/new.tsx');
 await mkdir(join(root,'apps/account/src'),{recursive:true});await mkdir(join(root,'packages'),{recursive:true});await writeFile(join(root,file),initial);
 const watcher=watchWorkspaceStyleSources(root,new Map([[file,initial]]),{refresh,report,delayMs:20});
 try{
  await writeFile(join(root,file),initial.replace('p-4','p-8'));
  await new Promise(done=>setTimeout(done,80));expect(refresh).not.toHaveBeenCalled();
  await writeFile(join(root,file),'import "@erpx/ui/new";'+initial);
  await vi.waitFor(()=>expect(refresh).toHaveBeenCalledTimes(1),{timeout:3000});
  expect(css).toContain('packages/ui/src/new.tsx');
  watcher.close();await writeFile(join(root,file),'import "./other";'+initial);
  await new Promise(done=>setTimeout(done,80));expect(refresh).toHaveBeenCalledTimes(1);expect(report).not.toHaveBeenCalled();
 }finally{watcher.close();await rm(root,{recursive:true,force:true});}
});
describe('workspace impact boundaries',()=>{
 it('preserves historical app identity across a folder rename and a deleted dependency',()=>{
  const original=sources(),next=sources();
  original.set('apps/apps/package.json','{"name":"@erpx/app-apps"}');
  original.set('apps/apps/src/app/page.tsx','import "@erpx/contracts/old-launcher";');
  original.set('packages/contracts/src/old-launcher.ts','export const label="Apps";');
  next.set('apps/launcher/package.json','{"name":"@erpx/app-apps"}');
  next.set('apps/launcher/src/app/page.tsx','export default function Page(){return "Apps";}');
  const base=createWorkspaceGraph(original),head=createWorkspaceGraph(next);
  expect(base.apps.get('apps')).toContain('packages/contracts/src/old-launcher.ts');
  const edited=new Map(original);edited.set('packages/contracts/src/old-launcher.ts','export const label="Changed";');
  expect(appRuntimeFingerprint(createWorkspaceGraph(edited),'apps')).not.toBe(appRuntimeFingerprint(base,'apps'));
  expect(head.apps.get('apps')).toContain('apps/launcher/src/app/page.tsx');
  const selected=selectCandidateSuites(head,base,suites,['packages/contracts/src/old-launcher.ts']);
  expect(selected.apps).toEqual(['apps']);
  expect(selected.removed).toEqual(['packages/contracts/src/old-launcher.ts']);
  next.set('apps/duplicate/package.json','{"name":"@erpx/app-apps"}');
  expect(()=>createWorkspaceGraph(next)).toThrow('WORKSPACE_APP_IDENTITY_DUPLICATE:@erpx/app-apps');
 });
 it('selects only the real consumers and does not expand app selection through common helpers',()=>{
  const graph=createWorkspaceGraph(sources());
  expect(selectWorkspaceSuites(graph,suites,{changed:['packages/contracts/src/money.ts']}).suites.map(x=>x.id)).toEqual(['finance']);
  expect(selectWorkspaceSuites(graph,suites,{apps:['people']}).suites.map(x=>x.id)).toEqual(['people']);
 });
 it('uses both revision graphs for deleted and renamed imports',()=>{
  const original=sources(),next=sources();next.delete('packages/contracts/src/money.ts');next.set('packages/contracts/src/currency.ts','export const currency="USD";');next.set('packages/contracts/src/finance.ts','import "./currency";');
  const selected=selectCandidateSuites(createWorkspaceGraph(next),createWorkspaceGraph(original),suites,['packages/contracts/src/money.ts','packages/contracts/src/currency.ts']);
  expect(selected.suites.map(x=>x.id)).toEqual(['finance']);expect(selected.removed).toEqual(['packages/contracts/src/money.ts']);
 });
 it('keeps runtime fingerprints independent of test and type-only inputs while invalidating assets',()=>{
  const original=sources(),next=sources();next.set('tests/unit/shared.testing/finance.test.ts','changed test');next.set('packages/contracts/src/person.ts','export type Person={name:string;age:number};');
  expect(appRuntimeFingerprint(createWorkspaceGraph(original),'finance')).toBe(appRuntimeFingerprint(createWorkspaceGraph(next),'finance'));
  next.set('public/brand/icon.svg','asset-sha256:two');expect(appRuntimeFingerprint(createWorkspaceGraph(original),'finance')).not.toBe(appRuntimeFingerprint(createWorkspaceGraph(next),'finance'));
 });
 it('stops on a missing workspace export or unregistered source instead of invoking acceptance',()=>{
  const files=sources();files.set('apps/finance/src/app/page.tsx','import "@erpx/contracts/missing";');
  expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_EXPORT_UNRESOLVED');
  const graph=createWorkspaceGraph(sources());expect(()=>selectCandidateSuites(graph,graph,suites,['unregistered-source.ts'])).toThrow('WORKSPACE_CHANGE_UNMAPPED');
 });
});

it('rejects application internals across application and shared-package boundaries',()=>{
  const files=sources();files.set('apps/finance/src/app/page.tsx','import "../../../people/src/app/page";');
  expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_APP_IMPORT_FORBIDDEN');
  files.set('apps/finance/src/app/page.tsx','import "@erpx/contracts/finance";');
  files.set('packages/contracts/src/finance.ts','import "../../../apps/people/src/app/page";');
  expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_APP_IMPORT_FORBIDDEN');
});
it('resolves only declared package subpath exports when a manifest is present',()=>{
 const files=sources();files.set('packages/contracts/package.json',JSON.stringify({exports:{'./finance':'./src/finance.ts','./people':'./src/people.ts'}}));
 expect(createWorkspaceGraph(files).imports.get('apps/finance/src/app/page.tsx')).toContain('packages/contracts/src/finance.ts');
 files.set('apps/finance/src/app/page.tsx','import "@erpx/contracts/money";');
 expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_EXPORT_UNDECLARED');
});

it('keeps generated app outputs and verification edits outside the global runtime identity',async()=>{
 const root=await mkdtemp(join(tmpdir(),'erpx-runtime-inputs-'));
 try {
  const files=['bun.lock','package.json','next.config.ts','tsconfig.base.json','postcss.config.mjs',
   'ops/macos/install-attachment-runtime.py','ops/macos/attachment-rollout.md','ops/macos/erpxctl.py','ops/macos/offsite.py','ops/macos/recovery_status.py',
   'apps/finance/src/app/page.tsx'];
  for(const file of files){await mkdir(dirname(join(root,file)),{recursive:true});await writeFile(join(root,file),'initial');}
  await mkdir(join(root,'ops/macos/clamav'),{recursive:true});
  await mkdir(join(root,'public/brand'),{recursive:true});await writeFile(join(root,'public/brand/icon.svg'),'brand');
  await symlink('../../public',join(root,'apps/finance/public'));
  const original=await runtimeSourceFingerprint(root,{NODE_ENV:'test'});
  for(const file of ['apps/finance/.build/server.js','apps/finance/.next/dev/types/routes.d.ts','apps/finance/.cache/typecheck.tsbuildinfo','apps/finance/node_modules/vendor/runtime.ts','apps/finance/__pycache__/runtime.ts','apps/finance/.env.local/private.ts','tests/unit/finance/example.test.ts']){
   await mkdir(dirname(join(root,file)),{recursive:true});await writeFile(join(root,file),'new generated or verification input');
  }
  expect(await runtimeSourceFingerprint(root,{NODE_ENV:'test'})).toBe(original);
  await writeFile(join(root,'apps/finance/src/app/page.tsx'),'changed runtime');
  expect(await runtimeSourceFingerprint(root,{NODE_ENV:'test'})).not.toBe(original);
 } finally { await rm(root,{recursive:true,force:true}); }
});

import { recoverGatewayApps, inspectProcessIdentity, assertRetiredLeasePortsAvailable, type GatewayProcessLease, type ProcessIdentity } from '../../../scripts/local/app-process-leases';
describe('development process ownership',()=>{
 const root='/owned/erpx',identity:ProcessIdentity={cwd:root,started:'Thu Oct 22 17:00:00 2026',command:`node ${root}/node_modules/next/dist/bin/next dev ${root}/apps/finance --port 3204 --hostname 127.0.0.1 --webpack`};
 const lease=():GatewayProcessLease=>({version:2,root,gatewayPid:90,apps:[{id:'finance',pid:100,port:3204,identity}]});
 function ports(entries=new Map<number,ProcessIdentity>([[100,identity]])){
  const stopped:number[]=[];
  return {stopped,identity:(pid:number)=>entries.get(pid)??null,stop:(pid:number)=>{stopped.push(pid);entries.delete(pid);},waitForExit:async()=>{}};
 }
 it('recovers a dead gateway only after checking the exact app command, cwd and process birth',async()=>{
  const p=ports();await recoverGatewayApps(lease(),root,p);expect(p.stopped).toEqual([100]);
 });
 it('uses the registered launcher directory for Apps recovery and refuses the removed directory without signaling it',async()=>{
  const current={...identity,command:`node ${root}/node_modules/next/dist/bin/next dev ${root}/apps/launcher --port 3216 --hostname 127.0.0.1 --turbopack`};
  const value:GatewayProcessLease={version:2,root,gatewayPid:90,apps:[{id:'apps',pid:100,port:3216,identity:current}]};
  const owned=ports(new Map([[100,current]]));await recoverGatewayApps(value,root,owned);expect(owned.stopped).toEqual([100]);
  const previous={...current,command:current.command.replace('/apps/launcher ','/apps/apps ')};
  const foreign=ports(new Map([[100,previous]]));
  await expect(recoverGatewayApps({...value,apps:[{...value.apps[0],identity:previous}]},root,foreign)).rejects.toThrow('OWNERSHIP_CHANGED');expect(foreign.stopped).toEqual([]);
 });
 it('leaves a live supervisor and all its apps alone',async()=>{
  const p=ports(new Map([[90,identity],[100,identity]]));await expect(recoverGatewayApps(lease(),root,p)).rejects.toThrow('OWNER_STILL_ALIVE');expect(p.stopped).toEqual([]);
 });
 it('refuses reused PIDs, foreign roots, reserved ports and old unproven app leases',async()=>{
  for(const changed of [{...identity,started:'new process'},{...identity,cwd:'/other/project'},{...identity,command:'node unrelated.js'}]){
   const p=ports(new Map([[100,changed]]));await expect(recoverGatewayApps(lease(),root,p)).rejects.toThrow('OWNERSHIP_CHANGED');expect(p.stopped).toEqual([]);
  }
  for(const changed of [{...lease(),root:'/other/project'},{...lease(),version:1},{...lease(),apps:[{...lease().apps[0],port:3000}]}]){
   const p=ports();await expect(recoverGatewayApps(changed,root,p)).rejects.toThrow();expect(p.stopped).toEqual([]);
  }
 });
 it('validates the entire lease before signaling any process',async()=>{
  const value=lease();value.apps.push({...value.apps[0],id:'people',pid:101,port:3205});
  const p=ports(new Map([[100,identity],[101,{...identity,cwd:'/foreign'}]]));await expect(recoverGatewayApps(value,root,p)).rejects.toThrow('OWNERSHIP_CHANGED');expect(p.stopped).toEqual([]);
 });
 it('does not inspect or signal PIDs from a proven old Linux namespace or boot',async()=>{
  const old={bootId:'11111111-1111-1111-1111-111111111111',namespace:'pid:[100]'},current={...old,namespace:'pid:[200]'};
  for(const now of [current,{...old,bootId:'22222222-2222-2222-2222-222222222222'}]){
   const p=ports(new Map([[90,identity],[100,identity]]));const probe=vi.spyOn(p,'identity');
   expect(await recoverGatewayApps({...lease(),version:3,pidNamespace:old},root,p,undefined,now)).toBe(true);
   expect(probe).not.toHaveBeenCalled();expect(p.stopped).toEqual([]);
  }
  const p=ports();await expect(recoverGatewayApps({...lease(),version:3,pidNamespace:old,root:'/foreign'},root,p,undefined,current)).rejects.toThrow('GATEWAY_LEASE_IDENTITY_REQUIRED');expect(p.stopped).toEqual([]);
  await expect(recoverGatewayApps({...lease(),version:3,pidNamespace:old,apps:[{...lease().apps[0],port:3000}]},root,p,undefined,current)).rejects.toThrow('INVALID_APP_PROCESS_LEASE');expect(p.stopped).toEqual([]);
  await expect(recoverGatewayApps({...lease(),version:3,pidNamespace:old,apps:[{...lease().apps[0],identity:{...identity,cwd:'/foreign'}}]},root,p,undefined,current)).rejects.toThrow('OWNERSHIP_CHANGED');expect(p.stopped).toEqual([]);
 });
 it('checks current and unproven namespaces with the existing strict identity rules',async()=>{
  const namespace={bootId:'11111111-1111-1111-1111-111111111111',namespace:'pid:[100]'},value={...lease(),version:3,pidNamespace:namespace};
  for(const current of [namespace,undefined]){const p=ports(new Map([[100,{...identity,started:'foreign'}]]));await expect(recoverGatewayApps(value,root,p,undefined,current)).rejects.toThrow('OWNERSHIP_CHANGED');expect(p.stopped).toEqual([]);}
  const p=ports(new Map([[90,identity]]));await expect(recoverGatewayApps(lease(),root,p,undefined,{...namespace,namespace:'pid:[200]'})).rejects.toThrow('OWNER_STILL_ALIVE');expect(p.stopped).toEqual([]);
 });
 it('refuses foreign listeners before replacing an old namespace lease',async()=>{
  for(const busy of [3100,3204]) await expect(assertRetiredLeasePortsAvailable(lease(),3100,async port=>port===busy)).rejects.toThrow('PORT_OCCUPIED');
  await expect(assertRetiredLeasePortsAvailable(lease(),3100,async()=>false)).resolves.toBeUndefined();
 });
 it('treats a disappeared process as absent and propagates other inspection failures',()=>{
  const failure=new Error('ps failed'),read=()=>{throw failure;};
  expect(inspectProcessIdentity(100,read,()=>{throw Object.assign(new Error('gone'),{code:'ESRCH'});})).toBeNull();
  expect(()=>inspectProcessIdentity(100,read,()=>{})).toThrow(failure);
  expect(()=>inspectProcessIdentity(100,read,()=>{throw Object.assign(new Error('denied'),{code:'EPERM'});})).toThrow(failure);
 });
 it('does not signal a process that has already exited',async()=>{
  const p=ports(new Map());await recoverGatewayApps(lease(),root,p);expect(p.stopped).toEqual([]);
 });
});

import { runtimeResourcePath } from '@erpx/server-runtime/public/runtime/resources';
it('resolves runtime resources from the composition root and refuses path traversal',()=>{
 const original=process.env.ERPX_RESOURCE_ROOT,repository=process.env.ERPX_REPOSITORY_ROOT;
 try {
  process.env.ERPX_RESOURCE_ROOT='/fixture/app-root';
  process.env.ERPX_REPOSITORY_ROOT='/different/root';
  expect(runtimeResourcePath('public/fonts/NotoSansThai.ttf')).toBe('/fixture/app-root/public/fonts/NotoSansThai.ttf');
  delete process.env.ERPX_RESOURCE_ROOT;
  expect(runtimeResourcePath('migrations/postgres')).toBe('/different/root/migrations/postgres');
  for(const path of ['../secret','public/../../secret','/etc/passwd','public\\secret','public//secret'])expect(()=>runtimeResourcePath(path)).toThrow('INVALID_RUNTIME_RESOURCE_PATH');
  process.env.ERPX_RESOURCE_ROOT='relative/root';expect(()=>runtimeResourcePath('public/font.ttf')).toThrow('ABSOLUTE_RUNTIME_RESOURCE_ROOT_REQUIRED');
 } finally {
  if(original===undefined)delete process.env.ERPX_RESOURCE_ROOT;else process.env.ERPX_RESOURCE_ROOT=original;
  if(repository===undefined)delete process.env.ERPX_REPOSITORY_ROOT;else process.env.ERPX_REPOSITORY_ROOT=repository;
 }
});

it('invalidates verification for assembled documentation paths without changing app runtime',()=>{
 const original=sources();original.set('tests/unit/shared.testing/finance.test.ts','readFileSync(resolve(root,"docs","status","example.json"));');original.set('docs/status/example.json','resource-sha256:before');
 const next=new Map(original);next.set('docs/status/example.json','resource-sha256:after');
 const a=createWorkspaceGraph(original),b=createWorkspaceGraph(next);
 expect(workspaceFingerprint(a,['tests/unit/shared.testing/finance.test.ts'])).not.toBe(workspaceFingerprint(b,['tests/unit/shared.testing/finance.test.ts']));
 expect(appRuntimeFingerprint(a,'finance')).toBe(appRuntimeFingerprint(b,'finance'));
 expect(selectCandidateSuites(b,a,suites,['docs/status/example.json']).suites.map(suite=>suite.id)).toEqual(['finance']);
});

it('tracks CSS imports and Tailwind text inputs without executing imports from scanned files',()=>{
 const files=sources();files.set('apps/finance/src/app/page.tsx','import "./globals.css";');
 files.set('apps/finance/src/app/globals.css','@import "@erpx/ui/components/tokens.css"; @source "./"; @source "../../../../packages/feature-finance/src";');
 files.set('packages/ui/src/components/tokens.css',':root{--brand:blue}');
 files.set('packages/feature-finance/src/panel.tsx','import "@erpx/server-finance/private"; export const Panel=()=> <div className="p-4"/>;');
 files.set('packages/server-finance/src/private.ts','export const query="initial";');
 const initial=createWorkspaceGraph(files),fingerprint=appRuntimeFingerprint(initial,'finance');
 const backend=new Map(files);backend.set('packages/server-finance/src/private.ts','changed server code');
 expect(appRuntimeFingerprint(createWorkspaceGraph(backend),'finance')).toBe(fingerprint);
 const ui=new Map(files);ui.set('packages/feature-finance/src/panel.tsx','export const Panel=()=> <div className="p-8"/>;');
 const graph=createWorkspaceGraph(ui);expect(appRuntimeFingerprint(graph,'finance')).not.toBe(fingerprint);
 expect(selectCandidateSuites(graph,initial,suites,['packages/feature-finance/src/panel.tsx']).apps).toEqual(['finance']);
 const tokens=new Map(files);tokens.set('packages/ui/src/components/tokens.css',':root{--brand:red}');
 expect(appRuntimeFingerprint(createWorkspaceGraph(tokens),'finance')).not.toBe(fingerprint);
 files.set('apps/finance/src/app/globals.css','@source "../../../people/src";');
 expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_APP_IMPORT_FORBIDDEN');
});

it('keys copied migration resources but keeps generated release metadata out of runtime identity', () => {
  const original = sources();
  original.set('packages/contracts/src/finance.ts', 'import "../../catalogue/src/release-artifact.json";');
  original.set('packages/catalogue/src/release-artifact.json', '{"softwareVersion":"old"}');
  original.set('migrations/postgres/control/0001.sql', 'CREATE TABLE original(id int);');
  const before = appRuntimeFingerprint(createWorkspaceGraph(original), 'finance');
  const metadata = new Map(original); metadata.set('packages/catalogue/src/release-artifact.json', '{"softwareVersion":"new"}');
  expect(appRuntimeFingerprint(createWorkspaceGraph(metadata), 'finance')).toBe(before);
  const migration = new Map(original); migration.set('migrations/postgres/control/0001.sql', 'CREATE TABLE changed(id int);');
  expect(appRuntimeFingerprint(createWorkspaceGraph(migration), 'finance')).not.toBe(before);
});

it('retains runtime edges when a facade exports both values and types from one implementation', () => {
  const files = sources();
  files.set('packages/contracts/src/finance.ts', 'export { amount } from "./money"; export type { Currency } from "./money";');
  files.set('packages/contracts/src/money.ts', 'export const amount=1; export type Currency=string;');
  const original = appRuntimeFingerprint(createWorkspaceGraph(files), 'finance');
  files.set('packages/contracts/src/money.ts', 'export const amount=2; export type Currency=string;');
  expect(appRuntimeFingerprint(createWorkspaceGraph(files), 'finance')).not.toBe(original);
});

import { maintenance } from '@erpx/server-runtime/public/runtime/maintenance';
it('reads only the owned maintenance marker without loading the worker composition', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'erpx-maintenance-'));
  try {
    expect(await maintenance(directory)).toBe(false);
    await writeFile(join(directory, 'maintenance'), '');
    expect(await maintenance(directory)).toBe(true);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

it('keeps verification-only package commands out of app runtime identities', () => {
  const files = sources();
  files.set('apps/finance/package.json', JSON.stringify({ name: '@erpx/app-finance', scripts: { typecheck: 'old-check', build: 'build-app' }, dependencies: { next: '16.4.0' } }));
  const before = appRuntimeFingerprint(createWorkspaceGraph(files), 'finance');
  files.set('apps/finance/package.json', JSON.stringify({ name: '@erpx/app-finance', scripts: { typecheck: 'new-check', build: 'build-app' }, dependencies: { next: '16.4.0' } }));
  expect(appRuntimeFingerprint(createWorkspaceGraph(files), 'finance')).toBe(before);
  files.set('apps/finance/package.json', JSON.stringify({ name: '@erpx/app-finance', scripts: { typecheck: 'new-check', build: 'build-app' }, dependencies: { next: '17.0.0' } }));
  expect(appRuntimeFingerprint(createWorkspaceGraph(files), 'finance')).not.toBe(before);
});

it('selects declared black-box boundary smoke without expanding into unrelated native suites', () => {
  const files = sources(); files.set('scripts/local/gateway.ts', 'export const version=1;');
  files.set('tests/e2e/shared.ui/boundary.spec.ts', 'export {};');
  const smoke: Suite = { ...suite('boundary-smoke', 'apps', 'tests/e2e/shared.ui/boundary.spec.ts'), lane: 'e2e', verificationInputs: ['scripts/local/gateway.ts'] };
  const graph = createWorkspaceGraph(files);
  expect(selectCandidateSuites(graph, graph, [...suites, smoke], ['scripts/local/gateway.ts']).suites.map(suite => suite.id)).toEqual(['boundary-smoke']);
});

it('retains lazy import and require edges alongside type-only imports of the same implementation', () => {
  for (const expression of ['import("./money")', 'require("./money")', 'import(`./money`)']) {
    const files = sources();
    files.set('packages/contracts/src/finance.ts', `import type { Currency } from "./money"; export const load=()=>${expression};`);
    const before = appRuntimeFingerprint(createWorkspaceGraph(files), 'finance');
    files.set('packages/contracts/src/money.ts', 'export const changed=1;');
    expect(appRuntimeFingerprint(createWorkspaceGraph(files), 'finance')).not.toBe(before);
  }
});

import { startWorkspaceBrowserRuntime } from '../../../scripts/testing/workspace-browser-runtime';
import { developmentHostProfile, workspaceBrowserApps, workspaceBrowserBundler } from '../../../scripts/testing/development-host-profile';
it('keeps development app profiles explicit and rejects compatibility mixed into product apps', async () => {
  expect(workspaceBrowserApps(undefined)).toBeNull();
  expect(workspaceBrowserApps('["account","apps","learn"]')).toEqual(['account', 'apps', 'learn']);
  expect(workspaceBrowserApps('["compat"]')).toEqual(['compat']);
  for (const value of ['[]', '["compat","learn"]', '["learn","learn"]', '["unknown"]']) expect(() => workspaceBrowserApps(value)).toThrow();
  expect(workspaceBrowserBundler(undefined)).toBe('webpack');
  expect(developmentHostProfile({apps:['account','learn'],entryApp:'learn',bundler:'turbopack'})).toEqual({apps:['account','learn'],entryApp:'learn',bundler:'turbopack'});
  expect(()=>developmentHostProfile({apps:['learn'],entryApp:'learn',bundler:'unknown' as 'webpack'})).toThrow('EXPLICIT_WORKSPACE_BROWSER_BUNDLER_REQUIRED');
  expect(()=>developmentHostProfile({apps:['learn'],entryApp:'finance'})).toThrow('DEVELOPMENT_HOST_ENTRY_APP_REQUIRED');
  expect(developmentHostProfile({apps:['account','finance'],entryApp:'finance',consoleRetired:true})).toEqual({apps:['account','finance'],entryApp:'finance',consoleRetired:true});
  expect(()=>developmentHostProfile({apps:['compat'],entryApp:'compat',consoleRetired:true})).toThrow('RETIRED_CONSOLE_PROFILE_REQUIRES_PRODUCT_HOSTS');
  await expect(startWorkspaceBrowserRuntime()).rejects.toThrow('ISOLATED_WORKSPACE_BROWSER_RUNTIME_REQUIRED');
});

import { runtimeLeafViolations } from '../../../scripts/architecture/runtime-leaves';
it('keeps storage and availability leaves from regaining indirect business-command dependencies', () => {
  const source='packages/server-runtime/src/adapters/runtime/filesystem.ts',read='packages/server-legal/src/adapters/runtime-mutation-authority.ts',command='packages/server-legal/src/adapters/retention-execution.ts';
  const runtimeImports=new Map([[source,new Set([read])],[read,new Set<string>()]]);
  expect(runtimeLeafViolations({runtimeImports})).toEqual([]);
  runtimeImports.get(read)!.add(command);
  expect(runtimeLeafViolations({runtimeImports})).toEqual([{source,target:command}]);
});

import { readFile, readdir, symlink } from 'node:fs/promises';
import { workspaceSourceCache } from '../../../scripts/testing/workspace-source-cache';
it('includes import-type queries in verification inputs without rebuilding application runtime', () => {
  const files=sources(),entry='packages/contracts/src/finance.ts',target='packages/contracts/src/money.ts';
  files.set(entry,'export type Amount = import("./money").Amount; export const currency="THB";');
  files.set(target,'export type Amount={minor:number};');
  const original=createWorkspaceGraph(files),beforeRuntime=appRuntimeFingerprint(original,'finance'),beforeTypes=workspaceFingerprint(original,[entry]);
  expect(original.imports.get(entry)?.has(target)).toBe(true);
  expect(original.runtimeImports.get(entry)?.has(target)).toBe(false);
  files.set(target,'export type Amount={minor:string};');
  const changed=createWorkspaceGraph(files);
  expect(appRuntimeFingerprint(changed,'finance')).toBe(beforeRuntime);
  expect(workspaceFingerprint(changed,[entry])).not.toBe(beforeTypes);
});
it('retains a real resource read alongside a type-only import of the same file', () => {
  const files=sources();files.set('apps/finance/src/app/page.tsx','import "../../../../scripts/local/reader";');
  files.set('scripts/local/reader.ts','import type { Amount } from "../../packages/contracts/src/money"; import {readFileSync} from "node:fs"; export const source=readFileSync(new URL("../../packages/contracts/src/money.ts",import.meta.url));');
  const before=appRuntimeFingerprint(createWorkspaceGraph(files),'finance');
  files.set('packages/contracts/src/money.ts','export type Amount=string; export const currency="USD";');
  expect(appRuntimeFingerprint(createWorkspaceGraph(files),'finance')).not.toBe(before);
});
it('includes external import-equals syntax as a runtime module dependency', () => {
  const files=sources();files.set('packages/contracts/src/finance.ts','import money = require("./money"); export const currency=money.currency;');
  expect(createWorkspaceGraph(files).runtimeImports.get('packages/contracts/src/finance.ts')?.has('packages/contracts/src/money.ts')).toBe(true);
});
it('reuses source syntax across invocations and invalidates by bytes rather than file timestamps', async () => {
  const root=await mkdtemp(join(tmpdir(),'erpx-source-cache-'));
  try{
    const first=workspaceSourceCache(root),file='packages/contracts/src/finance.ts';
    expect(first.parse(file,'export * from "./money";').modules).toEqual([{specifier:'./money',runtime:true}]);first.flush();
    expect(first.stats.persisted).toBe(true);
    const next=workspaceSourceCache(root);expect(next.stats.loaded).toBe(true);
    expect(next.parse(file,'export * from "./money";').modules).toEqual([{specifier:'./money',runtime:true}]);
    expect(next.stats).toMatchObject({hits:1,misses:0});
    expect(next.parse(file,'export * from "./person";').modules).toEqual([{specifier:'./person',runtime:true}]);
    expect(next.stats.misses).toBe(1);
  }finally{await rm(root,{recursive:true,force:true});}
});
it('reparses corrupted syntax cache entries and repairs the owned cache without hiding imports', async () => {
  const root=await mkdtemp(join(tmpdir(),'erpx-source-cache-'));
  try{
    const file='packages/contracts/src/finance.ts',source='export * from "./money";',cache=workspaceSourceCache(root);cache.parse(file,source);cache.flush();
    const path=join(root,'.scratch/dev/workspace-graph/source-syntax.json'),content=JSON.parse(await readFile(path,'utf8'));
    content.entries[0][1].modules=[];await writeFile(path,JSON.stringify(content));
    const repaired=workspaceSourceCache(root);expect(repaired.stats.loaded).toBe(false);
    expect(repaired.parse(file,source).modules).toEqual([{specifier:'./money',runtime:true}]);repaired.flush();
    const next=workspaceSourceCache(root);expect(next.stats.loaded).toBe(true);expect(next.parse(file,source).modules).toHaveLength(1);expect(next.stats.hits).toBe(1);
  }finally{await rm(root,{recursive:true,force:true});}
});
it('resolves warm cached imports against changed exports and refuses missing mappings', async () => {
  const root=await mkdtemp(join(tmpdir(),'erpx-source-cache-'));
  try{
    const files=sources(),cache=workspaceSourceCache(root),manifest={name:'@erpx/contracts',exports:{'./finance':'./src/finance.ts','./people':'./src/people.ts'}};
    files.set('packages/contracts/package.json',JSON.stringify(manifest));createWorkspaceGraph(files,cache.parse);cache.flush();
    const warm=workspaceSourceCache(root);manifest.exports['./finance']='./src/people.ts';files.set('packages/contracts/package.json',JSON.stringify(manifest));
    const changed=createWorkspaceGraph(files,warm.parse);
    expect(changed.imports.get('apps/finance/src/app/page.tsx')).toEqual(new Set(['packages/contracts/src/people.ts']));
    files.set('packages/contracts/package.json',JSON.stringify({name:'@erpx/contracts',exports:{'./people':'./src/people.ts'}}));
    expect(()=>createWorkspaceGraph(files,warm.parse)).toThrow('WORKSPACE_EXPORT_UNDECLARED');
    expect(warm.stats.hits).toBeGreaterThan(0);
  }finally{await rm(root,{recursive:true,force:true});}
});
it('does not follow an unrelated cache-directory symlink and still parses current source', async () => {
  const root=await mkdtemp(join(tmpdir(),'erpx-source-cache-')),outside=await mkdtemp(join(tmpdir(),'erpx-other-cache-'));
  try{
    await mkdir(join(root,'.scratch'));await symlink(outside,join(root,'.scratch/dev'));
    const cache=workspaceSourceCache(root);expect(cache.parse('source.ts','import "./money";').modules).toEqual([{specifier:'./money',runtime:true}]);cache.flush();
    expect(cache.stats.persisted).toBe(false);expect(await readdir(outside)).toEqual([]);
  }finally{await rm(root,{recursive:true,force:true});await rm(outside,{recursive:true,force:true});}
});
it('stops graph selection on an unresolved relative source instead of silently dropping its dependency', () => {
  const files=sources();files.delete('packages/contracts/src/money.ts');
  expect(()=>createWorkspaceGraph(files)).toThrow('WORKSPACE_IMPORT_UNRESOLVED:packages/contracts/src/finance.ts:./money');
});
it('rejects a graph cache override outside its repository before creating files', async () => {
  const root=await mkdtemp(join(tmpdir(),'erpx-source-cache-')),outside=await mkdtemp(join(tmpdir(),'erpx-other-cache-'));
  try{expect(()=>workspaceSourceCache(root,outside)).toThrow('WORKSPACE_CACHE_ROOT_OUTSIDE_REPOSITORY');expect(await readdir(outside)).toEqual([]);}
  finally{await rm(root,{recursive:true,force:true});await rm(outside,{recursive:true,force:true});}
});

import { workspaceTaskDefinition } from '../../../scripts/testing/workspace-tasks';
import { appDefinition } from '@erpx/contracts/apps';
it('separates type and lint inputs from runtime compiler inputs while retaining their validation dependency', () => {
  const config=workspaceTaskDefinition(createWorkspaceGraph(sources()),appDefinition('finance'));
  expect(config.tasks.typecheck.inputs).toContain('$TURBO_ROOT$/packages/contracts/src/person.ts');
  expect(config.tasks.build.inputs).not.toContain('$TURBO_ROOT$/packages/contracts/src/person.ts');
  for(const file of ['scripts/testing/app-typecheck.ts','scripts/testing/scoped-lint.ts','scripts/testing/workspace-imports.ts','scripts/testing/workspace-source-cache.ts']){
    expect(config.tasks.typecheck.inputs).toContain('$TURBO_ROOT$/'+file);
    expect(config.tasks.build.inputs).not.toContain('$TURBO_ROOT$/'+file);
  }
  expect(config.tasks.build.dependsOn).toEqual(['typecheck']);
  expect(config.tasks.build.inputs).toContain('$TURBO_ROOT$/scripts/local/isolated-app-build.ts');
  expect(config.tasks.build.inputs).toContain('$TURBO_ROOT$/scripts/assets/prepare-pdf-assets.ts');
  expect(config.tasks.build.inputs).toContain('$TURBO_ROOT$/scripts/local/workspace-settings.json');
});

it('selects the app whose compiler or host activation changed without expanding an idle-budget edit',()=>{
  const path='scripts/local/workspace-settings.json',files=sources(),settings={version:1,bundlers:{finance:'webpack'},defaultBundler:'webpack',activeApps:[] as string[],idleMinutes:10};
  files.set(path,JSON.stringify(settings));const base=createWorkspaceGraph(files);
  files.set(path,JSON.stringify({...settings,bundlers:{finance:'turbopack'},idleMinutes:20}));const head=createWorkspaceGraph(files);
  const selected=selectCandidateSuites(head,base,suites,[path]);expect(selected.apps).toEqual(['finance']);expect(selected.suites.map(suite=>suite.id)).toEqual(['finance']);
  files.set(path,JSON.stringify({...settings,bundlers:{finance:'turbopack'},idleMinutes:30}));
  expect(selectCandidateSuites(createWorkspaceGraph(files),head,suites,[path]).apps).toEqual([]);
  files.set(path,JSON.stringify({...settings,activeApps:['people']}));
  expect(selectCandidateSuites(createWorkspaceGraph(files),base,suites,[path]).apps).toEqual(['people']);
});
it('fails configured app selection when a compiler mapping is invalid or removed',()=>{
  const path='scripts/local/workspace-settings.json',files=sources();files.set(path,JSON.stringify({version:1,bundlers:{finance:'unknown'},activeApps:[]}));
  expect(()=>selectCandidateSuites(createWorkspaceGraph(files),createWorkspaceGraph(sources()),suites,[path])).toThrow('WORKSPACE_SETTINGS_MAPPING_REQUIRED');
  files.set(path,JSON.stringify({version:1,bundlers:{},activeApps:[]}));const base=createWorkspaceGraph(files);files.delete(path);
  expect(()=>selectCandidateSuites(createWorkspaceGraph(files),base,suites,[path])).toThrow('WORKSPACE_SETTINGS_MAPPING_REQUIRED');
});


describe('runtime owner composition cycles', () => {
 it('rejects an owner loop through public exports and a composition helper', () => {
  const graph={runtimeImports:new Map([
   ['packages/server-records/src/adapters/read.ts',new Set(['packages/server-support/src/public/read.ts'])],
   ['packages/server-support/src/public/read.ts',new Set(['packages/server-support/src/adapters/read.ts'])],
   ['packages/server-support/src/adapters/read.ts',new Set(['scripts/local/composition.ts'])],
   ['scripts/local/composition.ts',new Set(['packages/server-records/src/adapters/read.ts'])],
  ])};
  expect(runtimeOwnerCycles(graph)).toEqual([{owners:['server-records','server-support'],files:[...graph.runtimeImports.keys()].sort()}]);
 });
 it('allows explicit ports with reciprocal package usage on independent subpaths', () => {
  const graph={runtimeImports:new Map([
   ['packages/server-records/src/adapters/composition.ts',new Set(['packages/server-support/src/public/read-factory.ts'])],
   ['packages/server-support/src/public/read-factory.ts',new Set(['packages/server-records/src/domain/source.ts'])],
   ['packages/server-records/src/domain/source.ts',new Set<string>()],
  ])};
  expect(runtimeOwnerCycles(graph)).toEqual([]);
 });
 it('does not merge independent same-owner cycles into a cross-owner cycle', () => {
  const graph={runtimeImports:new Map([
   ['packages/server-records/src/adapters/a.ts',new Set(['packages/server-records/src/adapters/b.ts'])],
   ['packages/server-records/src/adapters/b.ts',new Set(['packages/server-records/src/adapters/a.ts','packages/server-support/src/adapters/a.ts'])],
   ['packages/server-support/src/adapters/a.ts',new Set(['packages/server-support/src/adapters/b.ts'])],
   ['packages/server-support/src/adapters/b.ts',new Set(['packages/server-support/src/adapters/a.ts'])],
  ])};
  expect(runtimeOwnerCycles(graph)).toEqual([]);
 });
});


describe('observed cold route compile metrics',()=>{
 it('retains the first redirect compilation rather than a later warm authenticated render',()=>{
  expect(firstRouteCompile('GET /other 200 in 10s (compile: 9s, render: 1s)\nGET /target?person=fixture 307 in 6.2s (compile: 5.1s, proxy: 2ms, render: 1s)\nGET /target 200 in 800ms (compile: 12ms, render: 788ms)','/target')).toEqual({route:'/target',status:307,requestMs:6200,compileMs:5100});
 });
 it('refuses route appearance without an observed compiler duration',()=>{
  expect(()=>firstRouteCompile('GET /target-child 200 in 20ms (compile: 2ms, render: 18ms)\nGET /target 200 in 20ms','/target')).toThrow('FIRST_COLD_ROUTE_COMPILATION_NOT_OBSERVED');
 });
});


it('binds the first ensure-page compiler span to the exact route rather than nearby paths or warm entries',()=>{
 expect(firstRouteEnsure([
  {inputPage:'/holdings/[holdingSlug]/home/child/page',timestamp:1,duration:900000},
  {inputPage:'/holdings/[holdingSlug]/home/page',timestamp:2,duration:3456000},
  {inputPage:'/holdings/[holdingSlug]/home/page',timestamp:3,duration:1000},
 ],'/holdings/simulation/home')).toMatchObject({inputPage:'/holdings/[holdingSlug]/home/page',ensurePageMs:3456});
 expect(()=>firstRouteEnsure([],'/missing')).toThrow('COLD_ROUTE_ENSURE_PAGE_SPAN_REQUIRED');
});


it('binds minimal fictional cohorts to declared development hosts without changing runtime authority',()=>{
 expect(workspaceBrowserFixtureProfile('money-people',['compat'],false)).toBe('money-people');
 expect(workspaceBrowserFixtureProfile('money-people',['account','apps','finance'],false)).toBe('money-people');
 expect(workspaceBrowserFixtureProfile('public-hosts',['account','apps','booking','forms'],true)).toBe('public-hosts');
 expect(()=>workspaceBrowserFixtureProfile('money-people',['account','apps','legal'],false)).toThrow('EXPLICIT_WORKSPACE_FIXTURE_PROFILE_REQUIRED');
 expect(()=>workspaceBrowserFixtureProfile('public-hosts',['compat'],false)).toThrow('EXPLICIT_WORKSPACE_FIXTURE_PROFILE_REQUIRED');
 expect(()=>workspaceBrowserFixtureProfile('anything',null,false)).toThrow('WORKSPACE_FIXTURE_PROFILE_REQUIRES_WORKSPACE');
});


it('Holding browser profile is explicit, isolated and keeps legacy origins unchanged',()=>{
 const apps=['account','apps','finance','people'];
 expect(workspaceHoldingProfile(undefined,apps,true,'engineering',3104)).toBe(false);
 expect(workspaceHoldingProfile('1',apps,true,'engineering',3104)).toBe(true);
 expect(()=>assertWorkspaceHoldingBinding(true,false)).toThrow('HOLDING_BROWSER_PROFILE_CHANGED');
 expect(()=>assertWorkspaceHoldingBinding(false,true)).toThrow('HOLDING_BROWSER_PROFILE_CHANGED');
 expect(()=>assertWorkspaceHoldingBinding(true,'true')).toThrow('HOLDING_BROWSER_PROFILE_CHANGED');
 expect(()=>assertWorkspaceHoldingBinding(false,undefined)).not.toThrow();
 for(const args of [['0',apps,true,'engineering',3104],['1',apps,false,'engineering',3104],['1',apps,true,'booking',3104],['1',apps,true,'engineering',3102],['1',['account'],true,'engineering',3104]] as const) expect(()=>workspaceHoldingProfile(args[0],args[1],args[2],args[3],args[4])).toThrow('EXPLICIT_HOLDING_BROWSER_PROFILE_REQUIRED');
 expect(workspaceBrowserOrigins(apps,true)).toEqual(['https://acme.jesadakorn.com','https://northstar.jesadakorn.com','https://erpx-internal.jesadakorn.com','https://account-dev-erpx.jesadakorn.com','https://console-dev-erpx.jesadakorn.com','https://finance-dev-erpx.jesadakorn.com']);
 expect(workspaceBrowserOrigins(['compat'],false)).toEqual(['https://console-dev-erpx.jesadakorn.com']);
});
