import{expect,type Page,type TestInfo}from '@playwright/test';
import{readFile,writeFile}from 'node:fs/promises';
import{join}from 'node:path';
import{execFileSync}from 'node:child_process';
import{Pool}from 'pg';
import{workspaceHmrCase}from './workspace-hmr';
import{firstRouteEnsure,type RouteEnsureSpan}from '../../../scripts/performance/route-compile-metrics';

/** Isolated same-source monolith/app route comparison, using normal fictional
 * sign-in. The installed gateway and retired Console mapping are never changed. */
export async function representativeRoute(page:Page,info:TestInfo,app:'finance'|'people'|'work'){
 const run=process.env.ERPX_DEMO_RUN!,directory=join(process.cwd(),'.runtime/browser',run),profile=JSON.parse(await readFile(join(directory,'workspace-runtime.json'),'utf8'));
 const monolith=profile.apps.length===1&&profile.apps[0]==='compat',processApp=monolith?'compat':app;
 expect(profile.fixtureProfile).toBe('money-people');expect(profile.consoleRetired).toBe(false);expect(profile.apps).toEqual(monolith?['compat']:['account','apps',app]);
 const fixture=await workspaceHmrCase(profile.apps),company=fixture.fixture.companies.desktop,user=company.users['workspace-owner'];
 const origin=monolith?'https://console-dev-erpx.jesadakorn.com':`https://${app}-dev-erpx.jesadakorn.com`,query=new URLSearchParams({routeBenchmark:'preserved'});
 if(app==='finance')query.set('chart',company.report.chartId);
 if(app==='people'){
  const manifest=JSON.parse(await readFile(join(directory,'databases.json'),'utf8')),url=new URL(manifest.databases.H001_DB);
  expect([url.hostname,url.port,url.username,url.pathname]).toEqual(['127.0.0.1','5432','erpx_test_admin',`/erpx_browser_${run}_h001_db`]);
  const db=new Pool({connectionString:url.href,max:1});try{const rows=await db.query("SELECT id FROM hr_engagements WHERE company_id=$1 AND person_id=$2 AND status='active'",[company.companyId,company.learning.personId]);expect(rows.rows).toHaveLength(1);query.set('person',rows.rows[0].id);}finally{await db.end();}
 }
 const path=user.workspaceBase+({finance:'/finance/reports/company',people:'/people-organization/people/directory/current',work:'/home'}[app]),errors:string[]=[],failures:string[]=[];let legacyNavigationReady=false;
 page.on('pageerror',error=>errors.push(error.message));page.on('response',response=>{const url=new URL(response.url());if(url.origin===origin&&url.pathname===`/api/v1${user.workspaceBase}/navigation`&&response.status()===200)legacyNavigationReady=true;if(url.origin===origin&&(url.pathname.startsWith('/api/')||url.pathname.startsWith('/_next/static/'))&&(response.status()===404||response.status()>=500))failures.push(response.status()+' '+url.pathname);});
 let peakAppRssBytes=0,peakWorkspaceRssBytes=0,samples=0,samplingError:unknown;
 const sample=async()=>{try{const lease=JSON.parse(await readFile(join(fixture.snapshot,'.scratch/dev/apps/leases.json'),'utf8'));expect({root:lease.root,gatewayPid:lease.gatewayPid}).toEqual({root:fixture.snapshot,gatewayPid:profile.pid});const ownedApp=lease.apps.find((value:{id:string})=>value.id===processApp);if(!ownedApp)return;const rows=execFileSync('ps',['-axo','pid=,ppid=,rss='],{encoding:'utf8'}).trim().split('\n').map(line=>line.trim().split(/\s+/).map(Number)),owned=new Set<number>([ownedApp.pid]);let changed=true;while(changed){changed=false;for(const[pid,parent]of rows)if(owned.has(parent)&&!owned.has(pid)){owned.add(pid);changed=true;}}peakAppRssBytes=Math.max(peakAppRssBytes,rows.filter(([pid])=>owned.has(pid)).reduce((sum,[,,rss])=>sum+rss*1024,0));const workspaceOwned=new Set<number>([profile.pid]);changed=true;while(changed){changed=false;for(const[pid,parent]of rows)if(workspaceOwned.has(parent)&&!workspaceOwned.has(pid)){workspaceOwned.add(pid);changed=true;}}peakWorkspaceRssBytes=Math.max(peakWorkspaceRssBytes,rows.filter(([pid])=>workspaceOwned.has(pid)).reduce((sum,[,,rss])=>sum+rss*1024,0));samples++;}catch(error){samplingError=error;}};
 let pending=Promise.resolve();const timer=setInterval(()=>{pending=pending.then(sample);},200),started=performance.now();
 try{
  await page.goto(origin+path+'?'+query,{timeout:120_000});
  await expect(page.getByRole('textbox',{name:'Work email',exact:true})).toBeVisible({timeout:120_000});await page.getByRole('textbox',{name:'Work email',exact:true}).fill(user.email);await page.locator('input[name="password"]').fill(fixture.fixture.password);await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await expect(page.getByRole('heading',{name:{finance:'Reports and statements',people:'People',work:'Today'}[app],exact:true,includeHidden:true})).toBeVisible({timeout:120_000});
  await page.waitForLoadState('load',{timeout:120_000});
  if(monolith){
   await expect(page.getByRole('navigation',{name:'Primary',exact:true}).getByRole('link',{name:'Today',exact:true})).toHaveAttribute('href',user.workspaceBase+'/home');
   await expect(page.getByRole('button',{name:'Switch workspace',exact:true})).toBeVisible();
   await expect.poll(()=>legacyNavigationReady,{timeout:120_000,message:'The owned legacy navigation API must complete normally'}).toBe(true);
  }else{const menus=page.getByRole('navigation',{name:'Product and workspace',exact:true});await expect(menus.getByRole('button')).toHaveCount(2,{timeout:120_000});}
  const boundary=page.locator('[data-work-company]').first();await expect(boundary).toHaveAttribute('data-work-company',company.companyId);await expect(boundary).toHaveAttribute('data-work-user',user.userId);await expect(boundary).toHaveAttribute('data-work-assignment',user.assignmentId);await expect(boundary).toHaveAttribute('data-work-stale','false');
  expect(new URL(page.url()).origin).toBe(origin);expect(new URL(page.url()).searchParams.get('routeBenchmark')).toBe('preserved');
  const routeAppearanceIncludingSignInMs=performance.now()-started;
  await sample();if(samplingError)throw samplingError;expect(samples).toBeGreaterThan(0);expect(peakAppRssBytes).toBeGreaterThan(0);expect(errors).toEqual([]);expect(failures).toEqual([]);
  const trace=await readFile(join(fixture.snapshot,'apps',processApp,'.next/dev/trace'),'utf8'),spans:RouteEnsureSpan[]=[];
  for(const line of trace.split('\n').filter(Boolean)){const events=JSON.parse(line);for(const event of Array.isArray(events)?events:[events])if(event.name==='ensure-page'&&typeof event.tags?.inputPage==='string')spans.push({inputPage:event.tags.inputPage,timestamp:event.timestamp,duration:event.duration});}
  const coldCompile=firstRouteEnsure(spans,path);
  await writeFile(info.outputPath('route-performance.json'),JSON.stringify({acceptance:false,layout:monolith?'monolith':'apps',app,processApp,bundler:profile.bundler,sourceFingerprint:profile.sourceFingerprint,route:path,coldCompile,routeAppearanceIncludingSignInMs,sampledPeakAppRssBytes:peakAppRssBytes,sampledPeakWorkspaceRssBytes:peakWorkspaceRssBytes,memorySampleIntervalMs:200,memorySamples:samples,scope:'first cold route and sampled owned process-tree RSS; no system-wide peak or production acceptance'},null,2)+'\n');
 }finally{clearInterval(timer);await pending;}
}
