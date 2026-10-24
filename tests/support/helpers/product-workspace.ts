import {expect,type Page,type BrowserContext,type TestInfo} from '@playwright/test';
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {Pool} from 'pg';
import {workspaceHmrCase} from './workspace-hmr';

export type PilotProduct='work'|'people'|'recruit';
export async function productWorkspaceCase(app:PilotProduct){
 const fixture=await workspaceHmrCase(['account','apps',app]),company=fixture.fixture.companies.desktop;
 const user=company.users[app==='work'?'finance-author':app==='people'?'hr':'talent-author'];
 const origin=`https://${app}-dev-erpx.jesadakorn.com`,query=new URLSearchParams({workspaceSmoke:'preserved'});
 if(app==='people'){
  const run=process.env.ERPX_DEMO_RUN!;
  const manifest=JSON.parse(await readFile(join(process.cwd(),'.runtime/browser',run,'databases.json'),'utf8')),url=new URL(manifest.databases.H001_DB);
  expect([url.hostname,url.port,url.username,url.pathname]).toEqual(['127.0.0.1','5432','erpx_test_admin',`/erpx_browser_${run}_h001_db`]);
  const db=new Pool({connectionString:url.href,max:1});
  try{
   const engagement=await db.query('SELECT id FROM hr_engagements WHERE company_id=$1 AND person_id=$2 AND status=$3',[company.companyId,company.learning.personId,'active']);
   expect(engagement.rows).toHaveLength(1);query.set('person',engagement.rows[0].id);
  }finally{await db.end();}
 }
 const path=app==='work'?'/home':app==='people'?'/people-organization/people/directory/current':'/people-organization/talent';
 return{app,fixture,company,user,origin,target:user.workspaceBase+path+'?'+query,title:app==='work'?'Today':app==='people'?'People':'Talent'};
}
export type ProductWorkspaceCase=Awaited<ReturnType<typeof productWorkspaceCase>>;
export async function signInProductWorkspace(page:Page,input:ProductWorkspaceCase){
 // People retains its existing responsive detail drawer; wide desktop detail
 // presentation is outside this bounded migration smoke.
 if(input.app==='people')await page.setViewportSize({width:1200,height:900});
 const login=await page.goto(input.origin+input.target,{timeout:120_000});
 expect(new URL(page.url()).origin).toBe('https://account-dev-erpx.jesadakorn.com');
 expect(login?.headers()['x-erpx-app']).toBe('account');
 await page.getByRole('textbox',{name:'Work email',exact:true}).fill(input.user.email);
 await page.locator('input[name="password"]').fill(input.fixture.fixture.password);
 const started=performance.now();
 await page.getByRole('button',{name:'Sign in',exact:true}).click();
 await expect(page.getByRole('heading',{name:input.title,exact:true,includeHidden:true})).toBeVisible({timeout:90_000});
 await page.waitForLoadState('load',{timeout:120_000});
 const menus=page.getByRole('navigation',{name:'Product and workspace',exact:true});
 await expect(menus.getByRole('button')).toHaveCount(2,{timeout:120_000});await expect(menus.getByRole('button').first()).toBeVisible();
 expect(new URL(page.url()).origin).toBe(input.origin);
 expect(new URL(page.url()).searchParams.get('workspaceSmoke')).toBe('preserved');
 for(const key of ['code','state','token','session_token'])expect(new URL(page.url()).searchParams.has(key)).toBe(false);
 return performance.now()-started;
}
export async function productWorkspaceDrawer(page:Page,input:ProductWorkspaceCase){
 if(input.app==='work'){
  await page.getByRole('button',{name:/Share an update/}).click();
  const drawer=page.getByRole('dialog',{name:'Share an update',exact:true});await expect(drawer).toBeVisible();
  const field=drawer.getByRole('combobox',{name:'Post message',exact:true});await expect(field).toBeEditable({timeout:30_000});
  return{drawer,field};
 }
 if(input.app==='recruit'){
  await page.getByRole('button',{name:'Draft requisition',exact:true}).click();
  const drawer=page.getByRole('dialog',{name:'Draft requisition',exact:true});await expect(drawer).toBeVisible();
  return{drawer,field:drawer.getByRole('textbox',{name:'Job title',exact:true})};
 }
 await page.getByRole('button',{name:'Edit profile',exact:true}).click();
 const field=page.getByRole('textbox',{name:'Legal name',exact:true});
 const drawer=page.getByRole('dialog').filter({has:field});await expect(drawer).toBeVisible({timeout:60_000});await expect(field).toBeEditable();
 return{drawer,field};
}
export async function verifyProductWorkspaceSmoke(page:Page,context:BrowserContext,info:TestInfo,app:PilotProduct){
 const input=await productWorkspaceCase(app),run=process.env.ERPX_DEMO_RUN!;
 const manifest=JSON.parse(await readFile(join(process.cwd(),'.runtime/browser',run,'databases.json'),'utf8')),url=new URL(manifest.databases.H001_DB);
 expect([url.hostname,url.port,url.username,url.pathname]).toEqual(['127.0.0.1','5432','erpx_test_admin',`/erpx_browser_${run}_h001_db`]);
 const db=new Pool({connectionString:url.href,max:1});
 const image=async()=>({
  assignments:(await db.query('SELECT * FROM assignments WHERE company_id=$1 AND user_id=$2 ORDER BY id',[input.company.companyId,input.user.userId])).rows,
  grants:(await db.query('SELECT * FROM company_package_grants WHERE company_id=$1 ORDER BY package_id',[input.company.companyId])).rows,
  people:(await db.query('SELECT id,legal_name,status,version FROM hr_people WHERE company_id=$1 ORDER BY id',[input.company.companyId])).rows,
  posts:(await db.query('SELECT id,state FROM social_posts WHERE company_id=$1 ORDER BY id',[input.company.companyId])).rows,
  requisitions:(await db.query('SELECT id,status,version FROM talent_requisitions WHERE company_id=$1 ORDER BY id',[input.company.companyId])).rows,
 });
 try{
  const before=await image(),errors:string[]=[],mutations:string[]=[],failedApiResponses:string[]=[];
  expect(before.grants.map(row=>row.package_id)).toEqual([app]);
  page.on('pageerror',error=>errors.push(error.message));
  // The normal shell marks its own Home feed read on mount.
  const homeReadMarker='/api/v1'+input.user.workspaceBase+'/navigation';
  page.on('request',request=>{const url=new URL(request.url());if(['POST','PUT','PATCH','DELETE'].includes(request.method())&&url.pathname.startsWith('/api/v1/holdings/')&&url.pathname!==homeReadMarker)mutations.push(request.method()+' '+url.pathname);});
  page.on('response',response=>{const url=new URL(response.url());if(url.origin===input.origin&&url.pathname.startsWith('/api/')&&(response.status()===404||response.status()>=500))failedApiResponses.push(response.status()+' '+url.pathname);});
  if(app==='work'){
   const anonymous=await page.goto(input.origin+'/api/v1/holdings/simulation/beta/home',{timeout:120_000});
   expect(anonymous?.status()).toBe(401);expect(anonymous?.headers()['x-erpx-app']).toBe('work');
  }
  await signInProductWorkspace(page,input);
  const{drawer}=await productWorkspaceDrawer(page,input),box=await drawer.boundingBox(),viewport=page.viewportSize()!;
  expect(box).not.toBeNull();expect(Math.abs(box!.x+box!.width-viewport.width)).toBeLessThan(2);expect(box!.x).toBeGreaterThan(viewport.width*0.2);
  await info.attach(`${app} isolated right drawer without submission`,{body:await page.screenshot(),contentType:'image/png'});
  if(app==='people')await drawer.getByRole('button',{name:'Close details',exact:true}).click();
  else await drawer.getByRole('button',{name:'Close dialog',exact:true}).click();
  await expect(drawer).toBeHidden();
  const response=await page.reload();expect(response?.status()).toBe(200);expect(response?.headers()['x-erpx-app']).toBe(app);
  await expect(page.getByRole('heading',{name:input.title,exact:true})).toBeVisible({timeout:60_000});
  const cookies=(await context.cookies()).filter(cookie=>['__Host-erpx-product','__Secure-erpx-account.session_token'].includes(cookie.name));
  expect(cookies.some(cookie=>cookie.domain===new URL(input.origin).hostname)).toBe(true);expect(cookies.some(cookie=>cookie.domain==='account-dev-erpx.jesadakorn.com')).toBe(true);
  for(const cookie of cookies)expect({parentDomain:cookie.domain.startsWith('.'),secure:cookie.secure,httpOnly:cookie.httpOnly}).toEqual({parentDomain:false,secure:true,httpOnly:true});
  expect(await image()).toEqual(before);expect(errors).toEqual([]);expect(mutations).toEqual([]);expect(failedApiResponses).toEqual([]);
 }finally{await db.end();}
}
