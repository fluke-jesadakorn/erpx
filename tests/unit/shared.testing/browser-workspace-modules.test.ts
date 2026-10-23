import { mkdtemp, mkdir, writeFile, readFile, symlink, realpath, rm, cp } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { expect, it, vi } from 'vitest';
import { linkBrowserWorkspaceModules } from '../../../scripts/testing/browser-workspace-modules';
import {seedWorkspaceProductPackages} from '../../../fixtures/shared/builders/workspace-product-packages';
import type {PostgresDatabase} from '@erpx/server-runtime/adapters/runtime/postgres';
import type {EngineeringMoneyPeopleBrowser} from '../../../fixtures/finance/builders/engineering-money-people-browser';

it('refuses product visibility fixtures outside their declared disposable database and profile before SQL',async()=>{
 const run='aaaaaaaaaaaa',owned=`postgres://erpx_test_admin@127.0.0.1:5432/erpx_browser_${run}_h001_db`,transaction=vi.fn();
 const cohort={run,companies:{desktop:{companyId:'synthetic-company'}}} as unknown as EngineeringMoneyPeopleBrowser;
 const attempt=(url:string,apps:string[]=['work'],fixture=cohort)=>seedWorkspaceProductPackages({run,apps,cohort:fixture,db:{pool:{options:{connectionString:url}},transaction} as unknown as PostgresDatabase});
 for(const url of [owned.replace('127.0.0.1','remote.invalid'),owned.replace('5432','5433'),owned.replace('erpx_test_admin','erpx_admin'),owned.replace(`erpx_browser_${run}_h001_db`,'erpx_h001_db'),owned.replace(run,'bbbbbbbbbbbb'),owned.replace('h001_db','control_db')])await expect(attempt(url)).rejects.toThrow('DISPOSABLE_PRODUCT_WORKSPACE_FIXTURE_REQUIRED');
 for(const apps of [[],['work','work'],['unregistered']])await expect(attempt(owned,apps)).rejects.toThrow('DISPOSABLE_PRODUCT_WORKSPACE_FIXTURE_REQUIRED');
 await expect(attempt(owned,['work'],{...cohort,run:'bbbbbbbbbbbb'})).rejects.toThrow('DISPOSABLE_PRODUCT_WORKSPACE_FIXTURE_REQUIRED');
 await expect(attempt(owned,['work'],{...cohort,companies:{}})).rejects.toThrow('DISPOSABLE_PRODUCT_WORKSPACE_FIXTURE_REQUIRED');
 expect(transaction).not.toHaveBeenCalled();
});

async function fixture(){
  const root=await realpath(await mkdtemp(join(tmpdir(),'erpx-workspace-modules-'))),template=join(root,'.runtime/browser/aaaaaaaaaaaa/app'),target=join(root,'.runtime/browser/bbbbbbbbbbbb/app');
  await mkdir(join(template,'packages/ui'),{recursive:true});await mkdir(join(target,'packages'),{recursive:true});
  for(const app of [template,target])await writeFile(join(app,'package.json'),JSON.stringify({name:'erpx'}));
  await writeFile(join(template,'packages/ui/package.json'),JSON.stringify({name:'@erpx/ui'}));
  await writeFile(join(template,'packages/ui/component.tsx'),'template source');
  await cp(join(template,'packages/ui'),join(target,'packages/ui'),{recursive:true});
  await mkdir(join(template,'node_modules/@erpx'),{recursive:true});await mkdir(join(template,'node_modules/react'));
  await writeFile(join(template,'node_modules/react/index.js'),'immutable third-party bytes');
  await symlink(join(template,'packages/ui'),join(template,'node_modules/@erpx/ui'),'dir');
  for(const app of [template,target]){
    await mkdir(join(app,'apps/learn'),{recursive:true});await writeFile(join(app,'apps/learn/package.json'),JSON.stringify({name:'@erpx/app-learn'}));
  }
  await symlink(join(template,'apps/learn'),join(template,'node_modules/@erpx/app-learn'),'dir');
  return{root,template,target};
}
it('resolves cloned workspace imports inside the case while retaining shared third-party bytes',async()=>{
  const f=await fixture();try{
    await linkBrowserWorkspaceModules(f.template,f.target);
    expect(await realpath(join(f.target,'node_modules'))).toBe(join(f.target,'node_modules'));
    expect(await realpath(join(f.target,'node_modules/@erpx/ui'))).toBe(join(f.target,'packages/ui'));
    expect(await realpath(join(f.target,'node_modules/@erpx/app-learn'))).toBe(join(f.target,'apps/learn'));
    expect(await realpath(join(f.target,'node_modules/react'))).toBe(join(f.template,'node_modules/react'));
    await writeFile(join(f.target,'packages/ui/component.tsx'),'case HMR edit');
    expect(await readFile(join(f.target,'node_modules/@erpx/ui/component.tsx'),'utf8')).toBe('case HMR edit');
    expect(await readFile(join(f.template,'packages/ui/component.tsx'),'utf8')).toBe('template source');
  }finally{await rm(f.root,{recursive:true,force:true});}
});
it('rejects a workspace alias escaping the owned template before creating case modules',async()=>{
  const f=await fixture();try{
    await mkdir(join(f.root,'foreign'));await rm(join(f.template,'node_modules/@erpx/ui'));await symlink(join(f.root,'foreign'),join(f.template,'node_modules/@erpx/ui'),'dir');
    await expect(linkBrowserWorkspaceModules(f.template,f.target)).rejects.toThrow('EXACT_BROWSER_WORKSPACE_ALIAS_REQUIRED');
    expect(existsSync(join(f.target,'node_modules'))).toBe(false);
  }finally{await rm(f.root,{recursive:true,force:true});}
});
it('refuses unrelated clone roots and never replaces an existing case installation',async()=>{
  const f=await fixture();try{
    await expect(linkBrowserWorkspaceModules(f.template,join(f.root,'unrelated'))).rejects.toThrow('OWNED_BROWSER_WORKSPACE_ROOTS_REQUIRED');
    await mkdir(join(f.target,'node_modules'));await writeFile(join(f.target,'node_modules/owned-marker'),'retain');
    await expect(linkBrowserWorkspaceModules(f.template,f.target)).rejects.toThrow('FRESH_BROWSER_NODE_MODULES_REQUIRED');
    expect(await readFile(join(f.target,'node_modules/owned-marker'),'utf8')).toBe('retain');
  }finally{await rm(f.root,{recursive:true,force:true});}
});
