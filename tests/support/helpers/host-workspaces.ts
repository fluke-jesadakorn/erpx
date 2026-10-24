import{workspaceHmrReady}from './workspace-hmr';
import{measureFrontWorkspaceHmr,measureSharedWorkspaceHmr}from './host-workspace-hmr';
import {expect,type Page,type BrowserContext,type TestInfo} from '@playwright/test';
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {Pool} from 'pg';

async function workspace(app:string){
 const run=process.env.ERPX_DEMO_RUN;
 if(process.env.ERPX_BROWSER_EXECUTION_MODE!=='development-smoke'||!run||!/^[a-f0-9]{12}$/.test(run))throw Error('EXACT_HOST_WORKSPACE_REQUIRED');
 const directory=join(process.cwd(),'.runtime/browser',run),runtime=JSON.parse(await readFile(join(directory,'workspace-runtime.json'),'utf8')),state=JSON.parse(await readFile(join(directory,'state.json'),'utf8'));
 expect(runtime).toMatchObject({run,acceptance:false,sourceRoot:join(directory,'app'),consoleRetired:true});expect(runtime.apps).toContain(app);expect(runtime.apps).not.toContain('compat');expect(state).toMatchObject({run,ready:true,executionMode:'development-smoke'});
 return {run,directory,runtime,state};
}
function observations(page:Page,origin:string){
 const errors:string[]=[],failures:string[]=[],mutations:string[]=[];
 page.on('pageerror',error=>errors.push(error.message));
 page.on('response',response=>{const u=new URL(response.url());if(u.origin===origin&&(u.pathname.startsWith('/api/')||u.pathname.startsWith('/_next/static/'))&&(response.status()>=500||response.status()===404))failures.push(response.status()+' '+u.pathname);});
 page.on('request',request=>{const u=new URL(request.url());if(u.origin===origin&&u.pathname.startsWith('/api/')&&!['GET','HEAD','OPTIONS'].includes(request.method()))mutations.push(request.method()+' '+u.pathname);});
 return {errors,failures,mutations};
}
async function refresh(page:Page,app:string){const response=await page.reload();expect(response?.status()).toBe(200);expect(response?.headers()['x-erpx-app']).toBe(app);}

export async function verifyFrontWorkspace(page:Page,context:BrowserContext,info:TestInfo,app:'pos'|'pms',hmr=false){
 const fixture=await workspace(app),hotel=fixture.state.fixture.hospitality,credentials=JSON.parse(await readFile(join(fixture.directory,'credentials.json'),'utf8'));
 const user=credentials.users.find((entry:{role:string})=>entry.role===(app==='pos'?'pos_manager':'pms_manager'));expect(user).toBeTruthy();
 const manifest=JSON.parse(await readFile(join(fixture.directory,'databases.json'),'utf8')),url=new URL(manifest.databases.H002_DB);
 expect([url.hostname,url.port,url.username,url.pathname]).toEqual(['127.0.0.1','5432','erpx_test_admin',`/erpx_browser_${fixture.run}_h002_db`]);
 const db=new Pool({connectionString:url.href,max:1}),origin=`https://${app}-dev-erpx.jesadakorn.com`,observed=observations(page,origin),ready=workspaceHmrReady(page,origin),started=performance.now();
 try{
  const company=(await db.query('SELECT slug FROM organization_nodes WHERE id=$1 AND kind=\'company\'',[hotel.companyId])).rows[0];expect(company?.slug).toBeTruthy();
  const authority=async()=>(await db.query('SELECT * FROM assignments WHERE company_id=$1 AND user_id=$2 ORDER BY id',[hotel.companyId,user.userId])).rows,before=await authority();
  await context.addCookies([{name:`erpx_front_language_${user.userId}`,value:'en',url:origin}]);
  const ssoStarts:string[]=[];page.on('request',request=>{const url=new URL(request.url());if(url.origin===origin&&url.pathname==='/sso/start')ssoStarts.push(request.resourceType());});
  const target=`/riverside-lantern/${encodeURIComponent(company.slug)}/holding/${app==='pos'?'sale':''}?workspaceSmoke=preserved`;
  await page.goto(origin+target,{timeout:120_000});await page.waitForURL(url=>url.origin==='https://account-dev-erpx.jesadakorn.com',{timeout:120_000});
  await page.getByRole('textbox',{name:'Work email',exact:true}).fill(user.email);await page.locator('input[name="password"]').fill(credentials.password);await page.getByRole('button',{name:'Sign in',exact:true}).click();
  await expect(page.getByRole('heading',{name:app==='pos'?'Point of sale':'A good day starts together',exact:true})).toBeVisible({timeout:120_000});expect(new URL(page.url()).origin).toBe(origin);expect(new URL(page.url()).searchParams.get('workspaceSmoke')).toBe('preserved');
  if(app==='pos')await page.locator('summary:visible').filter({hasText:/^Customer · /}).click();else await page.getByRole('button',{name:'Write a post',exact:true}).click();
  const drawer=page.getByRole('dialog',{name:app==='pos'?/^Customer · /:'Write a post',exact:app!=='pos'});await expect(drawer).toBeVisible();
  const box=await drawer.boundingBox(),viewport=page.viewportSize()!;expect(box).not.toBeNull();expect(Math.abs(box!.x+box!.width-viewport.width)).toBeLessThan(2);
  if(hmr)await measureFrontWorkspaceHmr(page,info,app,fixture.runtime.apps,ready,observed.errors,started);
  await info.attach(`${app} owned Front workspace drawer`,{body:await page.screenshot(),contentType:'image/png'});await drawer.getByRole('button',{name:'Close dialog',exact:true}).click();await expect(drawer).toBeHidden();
  expect(ssoStarts).toEqual(['document']);await refresh(page,app);
  // A first visit to the Front root must also use one document handoff, with
  // the existing normal Account session and no manufactured product cookie.
  await context.clearCookies({name:'__Host-erpx-product'});
  const callback=page.waitForResponse(response=>new URL(response.url()).origin===origin&&new URL(response.url()).pathname==='/sso/callback',{timeout:120_000});
  await page.goto(origin+'/?choose=1',{timeout:120_000});expect((await callback).status()).toBe(303);
  await page.waitForURL(url=>url.origin===origin&&!url.pathname.startsWith('/sso/')&&url.pathname!=='/login',{timeout:120_000});
  await expect(page.locator('h1').first()).toBeVisible({timeout:120_000});expect(ssoStarts).toEqual(['document','document']);await refresh(page,app);expect(await authority()).toEqual(before);expect(observed.errors).toEqual([]);expect(observed.failures).toEqual([]);
  expect(observed.mutations.filter(value=>!value.endsWith('/api/v1/workspace-preference'))).toEqual([]);
  for(const cookie of (await context.cookies()).filter(cookie=>cookie.name==='__Host-erpx-product'))expect({domain:cookie.domain,secure:cookie.secure,httpOnly:cookie.httpOnly}).toEqual({domain:new URL(origin).hostname,secure:true,httpOnly:true});
 }finally{await db.end();}
}

export async function verifySharedWorkspace(page:Page,info:TestInfo,app:'portal'|'legal-portal'|'forms'|'booking',hmr=false){
 const fixture=await workspace(app),origin=`https://${app}-dev-erpx.jesadakorn.com`,observed=observations(page,origin),heading={portal:'Customer Portal','legal-portal':'Legal Portal',forms:'Forms',booking:'Find a stay that feels right.'}[app],ready=workspaceHmrReady(page,origin),started=performance.now();
 await page.goto(origin+'/?workspaceSmoke=preserved',{timeout:120_000});await expect(page.getByRole('heading',{name:heading,exact:true})).toBeVisible({timeout:120_000});expect(new URL(page.url()).origin).toBe(origin);expect(new URL(page.url()).searchParams.get('workspaceSmoke')).toBe('preserved');await refresh(page,app);
 if(app==='portal'){
  await page.goto(origin+'/customer-portal/enroll#token=declared-fictional-workspace-token');await expect(page.getByRole('heading',{name:'Accept your Customer Portal invitation',exact:true})).toBeVisible();await expect(page).toHaveURL(origin+'/customer-portal/enroll');expect(await page.locator('body').innerText()).not.toContain('declared-fictional-workspace-token');await expect(page.getByRole('button',{name:'Accept while signed in',exact:true})).toBeVisible();
 }else if(app==='legal-portal'){
  await page.goto(origin+'/legal-portal/enroll');await expect(page.getByRole('heading',{name:'Accept your Legal invitation',exact:true})).toBeVisible();const handoff=page.getByText('Continue on ERPX Account',{exact:true});await expect(handoff).toHaveAttribute('aria-disabled','true');await expect(handoff).not.toHaveAttribute('href');
 }else if(app==='forms'){
  const form=JSON.parse(await readFile(join(fixture.directory,'workspace-form.json'),'utf8'));await page.goto(origin+`/forms/simulation/${encodeURIComponent(form.id)}?utm_source=workspace-smoke`);await expect(page.getByRole('heading',{name:'Fictional workspace inquiry',exact:true})).toBeVisible();await expect(page.getByLabel('Name',{exact:true})).toBeEditable();await expect(page.getByLabel('How can we contact you?',{exact:true})).toBeEditable();await expect(page.locator('[name="consent"]')).not.toBeChecked();await expect(page.getByRole('button',{name:'Send request',exact:true})).toBeVisible();
 }else{
  await page.getByRole('link',{name:'View hotel',exact:true}).click();await expect(page.getByLabel('Check-in',{exact:true})).toBeEditable();await expect(page.getByRole('button',{name:'Search availability',exact:true})).toBeVisible();await expect(page.getByRole('link',{name:'Manage booking',exact:true})).toBeVisible();
 }
 if(hmr)await measureSharedWorkspaceHmr(page,info,app,fixture.runtime.apps,ready,observed.errors,started);
 await info.attach(`${app} owned public workspace`,{body:await page.screenshot(),contentType:'image/png'});await refresh(page,app);expect(observed.errors).toEqual([]);expect(observed.failures).toEqual([]);expect(observed.mutations).toEqual([]);
}
