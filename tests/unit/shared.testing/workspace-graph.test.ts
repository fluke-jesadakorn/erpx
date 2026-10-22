import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { runtimeSourceFingerprint } from '../../../scripts/local/source-fingerprint';
import { describe, expect, it } from 'vitest';
import { createWorkspaceGraph, selectWorkspaceSuites, selectCandidateSuites, appRuntimeFingerprint, workspaceFingerprint } from '../../../scripts/testing/workspace-graph';
import type { Suite } from '../../../scripts/testing/catalogue';
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
describe('workspace impact boundaries',()=>{
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
  const original=await runtimeSourceFingerprint(root,{NODE_ENV:'test'});
  for(const file of ['apps/finance/.build/server.js','apps/finance/.next/dev/types/routes.d.ts','apps/finance/.cache/typecheck.tsbuildinfo','tests/unit/finance/example.test.ts']){
   await mkdir(dirname(join(root,file)),{recursive:true});await writeFile(join(root,file),'new generated or verification input');
  }
  expect(await runtimeSourceFingerprint(root,{NODE_ENV:'test'})).toBe(original);
  await writeFile(join(root,'apps/finance/src/app/page.tsx'),'changed runtime');
  expect(await runtimeSourceFingerprint(root,{NODE_ENV:'test'})).not.toBe(original);
 } finally { await rm(root,{recursive:true,force:true}); }
});

import { recoverGatewayApps, type GatewayProcessLease, type ProcessIdentity } from '../../../scripts/local/app-process-leases';
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
import { workspaceBrowserApps } from '../../../scripts/testing/development-host-profile';
it('keeps development app profiles explicit and rejects compatibility mixed into product apps', async () => {
  expect(workspaceBrowserApps(undefined)).toBeNull();
  expect(workspaceBrowserApps('["account","apps","learn"]')).toEqual(['account', 'apps', 'learn']);
  expect(workspaceBrowserApps('["compat"]')).toEqual(['compat']);
  for (const value of ['[]', '["compat","learn"]', '["learn","learn"]', '["unknown"]']) expect(() => workspaceBrowserApps(value)).toThrow();
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
  expect(config.tasks.build.inputs).not.toContain('$TURBO_ROOT$/scripts/local/workspace-settings.json');
});
