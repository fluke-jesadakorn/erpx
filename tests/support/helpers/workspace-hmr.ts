import { expect, type Locator, type Page, type Request, type TestInfo, type WebSocket } from '@playwright/test';
import { createHash } from 'node:crypto';
import { readFile, realpath, writeFile, rename, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import type { EngineeringMoneyPeopleBrowser } from '../../../fixtures/finance/builders/engineering-money-people-browser';

/** Editor-style atomic saves keep the benchmark from compiling an empty or
 * partially written module and mistaking that remount for normal Fast Refresh. */
export async function writeWorkspaceSource(file: string, contents: string) {
  const run=process.env.ERPX_DEMO_RUN;
  if(!run||!/^[a-f0-9]{12}$/.test(run)||!file.startsWith(join(process.cwd(),'.runtime/browser',run,'app/packages')+'/')||await realpath(file)!==file)throw Error('OWNED_HMR_SOURCE_REQUIRED');
  const temporary=file+'.workspace-hmr-'+crypto.randomUUID()+'.tmp';
  await writeFile(temporary,contents,{flag:'wx',mode:0o600});
  try {await rename(temporary,file);} finally {await rm(temporary,{force:true});}
}

export function workspaceHmrReady(page: Page, origin: string) {
  let current: WebSocket|undefined,ready=false;
  page.on('websocket',socket=>{
    const url=new URL(socket.url());
    if(url.host!==new URL(origin).host||url.pathname!=='/_next/hmr')return;
    current=socket;ready=false;
    socket.on('framereceived',frame=>{
      if(current!==socket)return;
      try {const message=JSON.parse(frame.payload.toString());if(['sync','turbopack-connected'].includes(message.type))ready=true;}catch{}
    });
    socket.on('close',()=>{if(current===socket)ready=false;});
  });
  return async()=>{
    // A streamed heading/drawer can appear before the initial server render
    // finishes. Warm edits must not overlap that document's pending DB reads.
    await page.waitForLoadState('load',{timeout:120_000});
    await expect.poll(()=>ready,{timeout:30_000,message:'Next HMR must synchronize before a warm source edit'}).toBe(true);
    await page.evaluate(()=>new Promise<void>(done=>requestAnimationFrame(()=>requestAnimationFrame(()=>done()))));
  };
}

export async function workspaceHmrCase(apps: string[]) {
  expect(process.env.ERPX_BROWSER_EXECUTION_MODE).toBe('development-smoke');
  const root=process.cwd(),run=process.env.ERPX_DEMO_RUN;
  expect(run).toMatch(/^[a-f0-9]{12}$/);
  const directory=join(root,'.runtime/browser',run!),snapshot=join(directory,'app');
  expect(await realpath(snapshot)).toBe(snapshot);
  const runtime=JSON.parse(await readFile(join(directory,'workspace-runtime.json'),'utf8'));
  expect({root:runtime.sourceRoot,run:runtime.run,acceptance:runtime.acceptance,apps:runtime.apps}).toEqual({root:snapshot,run,acceptance:false,apps});
  expect(['webpack','turbopack']).toContain(runtime.bundler);
  const fixture=JSON.parse(await readFile(join(directory,'engineering-money-people.json'),'utf8')) as EngineeringMoneyPeopleBrowser;
  return {snapshot,runtime,fixture};
}

export async function measureWorkspaceHmr(input: {
  page: Page; info: TestInfo; fixture: Awaited<ReturnType<typeof workspaceHmrCase>>;
  app: string; source: string; needle: string; label: string;
  replacement?: (label: string) => string;
  locator(label: string): Locator; assertState(label: string): Promise<void>;
  authenticatedRouteAppearanceMs: number; errors: string[]; scope: string;
  /** Collect timings for a broken baseline, then fail with its original state
   * error. Never emit a passing measurement for this diagnostic path. */
  collectFailedBaselineTiming?: boolean;
}) {
  const {page,info,fixture:{snapshot,runtime},app,needle,locator,assertState,errors}=input;
  expect(input.source).toMatch(/^packages\/[a-z0-9_-]+\/src\/.+\.tsx$/);
  const file=join(snapshot,input.source),packageName=input.source.split('/')[1];
  expect(await realpath(file)).toBe(file);
  expect(await realpath(join(snapshot,'node_modules/@erpx',packageName))).toBe(join(snapshot,'packages',packageName));
  const original=await readFile(file,'utf8');
  expect(original.split(needle)).toHaveLength(2);
  const timeOrigin=await page.evaluate(()=>performance.timeOrigin),samples:number[]=[];
  let reloads=0,peakAppRssBytes=0;
  const navigation=(request:Request)=>{if(request.isNavigationRequest()&&request.frame()===page.mainFrame())reloads++;};
  page.on('request',navigation);
  async function memory() {
    const lease=JSON.parse(await readFile(join(snapshot,'.scratch/dev/apps/leases.json'),'utf8'));
    expect({root:lease.root,gateway:lease.gatewayPid}).toEqual({root:snapshot,gateway:runtime.pid});
    const appLease=lease.apps.find((item:{id:string})=>item.id===app);expect(appLease).toBeTruthy();
    const rows=execFileSync('ps',['-axo','pid=,ppid=,rss='],{encoding:'utf8'}).trim().split('\n').map(line=>line.trim().split(/\s+/).map(Number));
    const owned=new Set<number>([appLease.pid]);let changed=true;
    while(changed){changed=false;for(const[pid,parent]of rows)if(owned.has(parent)&&!owned.has(pid)){owned.add(pid);changed=true;}}
    peakAppRssBytes=Math.max(peakAppRssBytes,rows.filter(([pid])=>owned.has(pid)).reduce((sum,[,,rss])=>sum+rss*1024,0));
  }
  const change=(label:string)=>writeWorkspaceSource(file,original.replace(needle,input.replacement?.(label)??JSON.stringify(label)));
  let failed=false,failure:unknown,stateFailure:unknown;
  async function checkState(label:string){
    if(stateFailure)return;
    try{await assertState(label);}catch(error){if(!input.collectFailedBaselineTiming)throw error;stateFailure=error;}
  }
  try {
    const warmup=input.label+' · HMR warmup';await change(warmup);
    await expect(locator(warmup)).toBeVisible({timeout:30_000});await checkState(warmup);await memory();
    for(let index=0;index<20;index++){
      const label=`${input.label} · HMR ${index+1}`,started=performance.now();await change(label);
      await expect(locator(label)).toBeVisible({timeout:15_000});samples.push(performance.now()-started);
      expect(await page.evaluate(()=>performance.timeOrigin)).toBe(timeOrigin);
      await checkState(label);await memory();
    }
    if(stateFailure)throw stateFailure;
  } catch(error) {failed=true;failure=error;}
  try {
    await writeWorkspaceSource(file,original);expect(await readFile(file,'utf8')).toBe(original);
    // A failed editor-state assertion must remain the reported cause. Restore
    // source bytes regardless; require the original UI after successful runs.
    if(!failed)await expect(locator(input.label)).toBeVisible({timeout:30_000});
  } catch(cleanup) {
    if(failed)throw new AggregateError([failure,cleanup],'HMR measurement and source restoration failed');
    throw cleanup;
  } finally {page.off('request',navigation);}
  if(failed){
    await info.attach('workspace HMR failure diagnostics',{body:JSON.stringify({acceptance:false,functional:false,failedBaselineTiming:input.collectFailedBaselineTiming===true,sourceFingerprint:runtime.sourceFingerprint,componentSha256:createHash('sha256').update(original).digest('hex'),app,bundler:runtime.bundler,completedUpdates:samples.length,samples,...(samples.length===20?{p95Ms:[...samples].sort((a,b)=>a-b)[Math.ceil(samples.length*.95)-1]}:{}),reloads,errors,documentTimeOriginUnchanged:await page.evaluate(()=>performance.timeOrigin).catch(()=>null)===timeOrigin}),contentType:'application/json'});
    throw failure;
  }
  const sorted=[...samples].sort((a,b)=>a-b),p95Ms=sorted[Math.ceil(sorted.length*0.95)-1];
  const result={scope:input.scope,app,acceptance:false,bundler:runtime.bundler,sourceFingerprint:runtime.sourceFingerprint,componentSha256:createHash('sha256').update(original).digest('hex'),authenticatedRouteAppearanceMs:input.authenticatedRouteAppearanceMs,samples,p95Ms,peakAppRssBytes,reloads,errors,target:p95Ms<=3000};
  const path=info.outputPath('workspace-performance.json');await writeFile(path,JSON.stringify(result,null,2)+'\n');
  await info.attach('workspace-performance',{path,contentType:'application/json'});
  expect(samples).toHaveLength(20);expect(reloads).toBe(0);expect(errors).toEqual([]);
}
