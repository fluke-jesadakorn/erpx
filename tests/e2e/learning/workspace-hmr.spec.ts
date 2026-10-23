import { expect, test, type Request } from '@playwright/test';
import { createHash } from 'node:crypto';
import { readFile, realpath, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { browserProjects } from '../../../scripts/testing/browser-results';
import type { EngineeringMoneyPeopleBrowser } from '../../../fixtures/finance/builders/engineering-money-people-browser';

test('Learn client edits preserve the draft through twenty measured HMR updates', browserProjects(['desktop'], 'Controlled desktop development HMR benchmark; viewport acceptance remains deferred.'), async ({page}, info) => {
  test.setTimeout(240_000);
  expect(process.env.ERPX_BROWSER_EXECUTION_MODE).toBe('development-smoke');
  const root=process.cwd(),run=process.env.ERPX_DEMO_RUN;
  expect(run).toMatch(/^[a-f0-9]{12}$/);
  const directory=join(root,'.runtime/browser',run!),snapshot=join(directory,'app');
  expect(await realpath(snapshot)).toBe(snapshot);
  const runtime=JSON.parse(await readFile(join(directory,'workspace-runtime.json'),'utf8'));
  expect({root:runtime.sourceRoot,run:runtime.run,acceptance:runtime.acceptance,apps:runtime.apps}).toEqual({root:snapshot,run,acceptance:false,apps:['account','apps','learn']});
  expect(['webpack','turbopack']).toContain(runtime.bundler);
  const fixture=JSON.parse(await readFile(join(directory,'engineering-money-people.json'),'utf8')) as EngineeringMoneyPeopleBrowser;
  const company=fixture.companies.desktop,user=company.users.learner;
  const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));
  await page.goto(`https://learn-dev-erpx.jesadakorn.com${user.workspaceBase}/knowledge/learning/assignments/${company.learning.assignmentId}`,{timeout:120_000});
  expect(new URL(page.url()).origin).toBe('https://account-dev-erpx.jesadakorn.com');
  await page.getByRole('textbox',{name:'Work email',exact:true}).fill(user.email);
  await page.locator('input[name="password"]').fill(fixture.password);
  const routeStarted=performance.now();
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
  const heading=page.getByRole('heading',{name:'Start course',exact:true});
  await expect(heading).toBeVisible({timeout:90_000});
  const authenticatedRouteAppearanceMs=performance.now()-routeStarted;
  const draft='Fictional draft retained across measured client edits.';
  const reason=page.locator('form').filter({has:heading}).locator('[name="reason"]');
  await reason.fill(draft);
  // Only the owned disposable copy is ever edited; restore it before teardown.
  const file=join(snapshot,'packages/feature-learning/src/components/learning-console.tsx');
  expect(await realpath(file)).toBe(file);
  expect(await realpath(join(snapshot,'node_modules/@erpx/feature-learning'))).toBe(join(snapshot,'packages/feature-learning'));
  const original=await readFile(file,'utf8'),needle='<h2>Start course</h2>';
  expect(original.split(needle)).toHaveLength(2);
  let reloads=0,peakAppRssBytes=0;
  const timeOrigin=await page.evaluate(()=>performance.timeOrigin);
  const navigation=(request:Request)=>{if(request.isNavigationRequest()&&request.frame()===page.mainFrame())reloads++;};
  page.on('request',navigation);
  async function memory(){
    const lease=JSON.parse(await readFile(join(snapshot,'.scratch/dev/apps/leases.json'),'utf8'));
    expect({root:lease.root,gateway:lease.gatewayPid}).toEqual({root:snapshot,gateway:runtime.pid});
    const app=lease.apps.find((item:{id:string})=>item.id==='learn');expect(app).toBeTruthy();
    const rows=execFileSync('ps',['-axo','pid=,ppid=,rss='],{encoding:'utf8'}).trim().split('\n').map(line=>line.trim().split(/\s+/).map(Number));
    const owned=new Set<number>([app.pid]);let changed=true;
    while(changed){changed=false;for(const[pid,parent]of rows)if(owned.has(parent)&&!owned.has(pid)){owned.add(pid);changed=true;}}
    peakAppRssBytes=Math.max(peakAppRssBytes,rows.filter(([pid])=>owned.has(pid)).reduce((sum,[,,rss])=>sum+rss*1024,0));
  }
  const samples:number[]=[];
  try{
    await writeFile(file,original.replace(needle,'<h2>Start course · HMR warmup</h2>'));
    await expect(page.getByRole('heading',{name:'Start course · HMR warmup',exact:true})).toBeVisible({timeout:30_000});
    await memory();
    for(let index=0;index<20;index++){
      const label=`Start course · HMR ${index+1}`,started=performance.now();
      await writeFile(file,original.replace(needle,`<h2>${label}</h2>`));
      await expect(page.getByRole('heading',{name:label,exact:true})).toBeVisible({timeout:15_000});
      samples.push(performance.now()-started);
      expect(await page.evaluate(()=>performance.timeOrigin)).toBe(timeOrigin);
      await expect(page.locator('form').filter({has:page.getByRole('heading',{name:label,exact:true})}).locator('[name="reason"]')).toHaveValue(draft);
      await memory();
    }
  }finally{
    await writeFile(file,original);
    await expect(heading).toBeVisible({timeout:30_000});
    page.off('request',navigation);
    expect(await readFile(file,'utf8')).toBe(original);
  }
  const sorted=[...samples].sort((a,b)=>a-b),p95Ms=sorted[Math.ceil(sorted.length*0.95)-1];
  const result={scope:'Learn desktop client HMR; sampled process-tree RSS; excludes fixture preparation',acceptance:false,bundler:runtime.bundler,sourceFingerprint:runtime.sourceFingerprint,componentSha256:createHash('sha256').update(original).digest('hex'),authenticatedRouteAppearanceMs,samples,p95Ms,peakAppRssBytes,reloads,errors,target:p95Ms<=3000};
  const path=info.outputPath('workspace-performance.json');await writeFile(path,JSON.stringify(result,null,2)+'\n');
  await info.attach('workspace-performance',{path,contentType:'application/json'});
  expect(samples).toHaveLength(20);expect(reloads).toBe(0);expect(errors).toEqual([]);
});
