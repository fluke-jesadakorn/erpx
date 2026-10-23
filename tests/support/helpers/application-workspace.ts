import {expect,type Page,type BrowserContext,type TestInfo,type Locator} from '@playwright/test';
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {Pool} from 'pg';
import {workspaceHmrCase,workspaceHmrReady,measureWorkspaceHmr} from './workspace-hmr';
import {applicationHmrEntries} from './application-hmr';
import {productIds} from '@erpx/contracts/product-registry';

export type ApplicationSmokeApp='finance'|'sales'|'supply'|'assets'|'projects'|'support'|'service'|'legal'|'manufacturing';
const entries={finance:{path:'finance/reports/company',heading:'Reports and statements'},sales:{path:'customers-sales/sales/pipeline',heading:'Sales'},supply:{path:'purchasing/orders',heading:'Purchase orders'},assets:{path:'services/assets/register',heading:'Assets & maintenance'},projects:{path:'projects-routines/projects',heading:'Projects'},support:{path:'customers-sales/customers/service/cases',heading:'Customers'},service:{path:'services/field-service',heading:'Field service'},legal:{path:'services/legal/mine',heading:'Legal'},manufacturing:{path:'production/manufacturing',heading:'Manufacturing'}} as const;

/** Existing module/role authority without a Commercial purchase. Only the exact
 * owned development snapshot runs with Console retired; no installed flags. */
export async function verifyApplicationWorkspace(page:Page,context:BrowserContext,info:TestInfo,app:ApplicationSmokeApp,hmr=false){
 const runId=process.env.ERPX_DEMO_RUN;if(!runId||!/^[a-f0-9]{12}$/.test(runId))throw Error('EXACT_APPLICATION_RUN_REQUIRED');
 const profile=JSON.parse(await readFile(join(process.cwd(),'.runtime/browser',runId,'workspace-runtime.json'),'utf8'));
 expect(profile.apps).toEqual(expect.arrayContaining(['account','apps',app]));expect(profile.apps.every((id:string)=>(['account','apps',...productIds] as readonly string[]).includes(id))).toBe(true);
 const fixture=await workspaceHmrCase(profile.apps);expect(fixture.runtime.consoleRetired).toBe(true);
 const company=fixture.fixture.companies.desktop,user=company.users['workspace-owner'];expect(user).toBeTruthy();
 const run=process.env.ERPX_DEMO_RUN!,manifest=JSON.parse(await readFile(join(process.cwd(),'.runtime/browser',run,'databases.json'),'utf8')),url=new URL(manifest.databases.H001_DB);
 expect([url.hostname,url.port,url.username,url.pathname]).toEqual(['127.0.0.1','5432','erpx_test_admin',`/erpx_browser_${run}_h001_db`]);
 const db=new Pool({connectionString:url.href,max:1}),origin=`https://${app}-dev-erpx.jesadakorn.com`,entry=entries[app],query=new URLSearchParams({workspaceSmoke:'preserved'});
 if(app==='finance')query.set('chart',company.report.chartId);
 const target=user.workspaceBase+'/'+entry.path+'?'+query;
 const authority=async()=>({assignments:(await db.query('SELECT * FROM assignments WHERE company_id=$1 AND user_id=$2 ORDER BY id',[company.companyId,user.userId])).rows,
  grants:(await db.query('SELECT * FROM company_package_grants WHERE company_id=$1 ORDER BY package_id',[company.companyId])).rows,
  events:(await db.query('SELECT * FROM company_package_grant_events WHERE company_id=$1 ORDER BY id',[company.companyId])).rows});
 const errors:string[]=[],apiFailures:string[]=[],commands:string[]=[],ready=workspaceHmrReady(page,origin),started=performance.now();
 page.on('pageerror',error=>errors.push(error.message));
 page.on('response',response=>{const url=new URL(response.url());if(url.origin===origin&&url.pathname.startsWith('/api/')&&(response.status()===404||response.status()>=500))apiFailures.push(response.status()+' '+url.pathname);});
 page.on('request',request=>{const url=new URL(request.url());if(url.origin===origin&&url.pathname.startsWith('/api/v1/holdings/')&&['POST','PUT','PATCH','DELETE'].includes(request.method()))commands.push(request.method()+' '+url.pathname);});
 try{
  const before=await authority();expect(before.grants).toEqual([]);expect(before.events).toEqual([]);
  await page.goto(origin+target,{timeout:120_000});expect(new URL(page.url()).origin).toBe('https://account-dev-erpx.jesadakorn.com');
  await page.getByRole('textbox',{name:'Work email',exact:true}).fill(user.email);await page.locator('input[name="password"]').fill(fixture.fixture.password);
  await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await expect(page.getByRole('heading',{name:entry.heading,exact:true})).toBeVisible({timeout:120_000});
  expect(new URL(page.url()).origin).toBe(origin);expect(new URL(page.url()).searchParams.get('workspaceSmoke')).toBe('preserved');
  for(const key of ['code','state','token','session_token'])expect(new URL(page.url()).searchParams.has(key)).toBe(false);
  const owner=page.locator('[data-work-company]').first();await expect(owner).toHaveAttribute('data-work-company',company.companyId);await expect(owner).toHaveAttribute('data-work-user',user.userId);
  let drawer:Locator;
  if(app==='finance'){
   await page.getByRole('button',{name:'Prepare statement edition',exact:true}).click();drawer=page.getByRole('dialog',{name:'Prepare statement edition',exact:true});
   await expect(drawer.getByLabel('Current start date',{exact:true})).toBeEditable({timeout:60_000});
  }else if(app==='sales'){
   await page.getByRole('link',{name:'New lead',exact:true}).click();drawer=page.getByRole('dialog',{name:'New lead',exact:true});
  }else if(app==='supply'){
   await page.getByRole('link',{name:'New document',exact:true}).click();drawer=page.getByRole('dialog',{name:'Order details',exact:true});
  }else{
   const controls={assets:{role:'button',name:'Register asset'},projects:{role:'summary',name:'New project'},support:{role:'link',name:'New service case'},service:{role:'summary',name:'Create service order'},legal:{role:'button',name:'New request'},manufacturing:{role:'button',name:'Draft BOM or formula'}} as const;
   const control=controls[app];if(control.role==='summary')await page.locator('summary:visible').filter({hasText:new RegExp('^'+control.name+'$')}).click();else await page.getByRole(control.role,{name:control.name,exact:true}).click();drawer=page.getByRole('dialog',{name:control.name,exact:true});
  }
  await expect(drawer).toBeVisible({timeout:60_000});const box=await drawer.boundingBox(),viewport=page.viewportSize()!;
  expect(box).not.toBeNull();expect(Math.abs(box!.x+box!.width-viewport.width)).toBeLessThan(2);
  if(hmr){
   await ready();const spec=applicationHmrEntries[app],selector='input:not([type="hidden"]):not([type="checkbox"]):not([type="radio"]):not([type="file"]):not([readonly]):not([disabled]),textarea:not([readonly]):not([disabled])',field=drawer.locator(selector).first();
   await expect(field).toBeEditable();const type=await field.getAttribute('type'),original=await field.inputValue(),draft=type==='date'?(original==='2026-10-01'?'2026-10-02':'2026-10-01'):type==='number'?'2':type==='email'?'hmr@example.test':'Workspace HMR retained draft';await field.fill(draft);
   await measureWorkspaceHmr({page,info,fixture,app,source:spec.source,needle:spec.needle,label:spec.label,replacement:spec.replace,locator:label=>page.getByRole('dialog',{name:label,exact:true}),assertState:async label=>{await expect(page.getByRole('dialog',{name:label,exact:true}).locator(selector).first()).toHaveValue(draft);},authenticatedRouteAppearanceMs:performance.now()-started,errors,scope:app+' desktop owner drawer HMR'});
   await field.fill(original);
  }
  await info.attach(`${app} module-authorized workspace drawer`,{body:await page.screenshot(),contentType:'image/png'});
  await drawer.getByRole('button',{name:'Close dialog',exact:true}).click();if(hmr){const confirmation=page.getByRole('dialog',{name:/^(?:Discard changes\?|Discard unsaved changes\?|Unsaved changes)$/});if(await confirmation.isVisible())await confirmation.getByRole('button',{name:/^(?:Discard changes|Discard draft|Close without saving)$/}).click();}await expect(drawer).toBeHidden();
  const response=await page.reload();expect(response?.status()).toBe(200);expect(response?.headers()['x-erpx-app']).toBe(app);
  const cookies=(await context.cookies()).filter(cookie=>['__Host-erpx-product','__Secure-erpx-account.session_token'].includes(cookie.name));
  expect(cookies.some(cookie=>cookie.domain===new URL(origin).hostname)).toBe(true);
  for(const cookie of cookies)expect({parentDomain:cookie.domain.startsWith('.'),secure:cookie.secure,httpOnly:cookie.httpOnly}).toEqual({parentDomain:false,secure:true,httpOnly:true});
  expect(await authority()).toEqual(before);expect(errors).toEqual([]);expect(apiFailures).toEqual([]);expect(commands).toEqual([]);
 }finally{await db.end();}
}
