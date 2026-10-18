import {expect,test,type Page} from '@playwright/test';
import {readFileSync} from 'node:fs';
const base='/holdings/acme';
const origin='https://console-dev-erpx.jesadakorn.com';
test.setTimeout(600_000);
async function signIn(page:Page){
 const credentials=JSON.parse(readFileSync('.scratch/dev/credentials/console-dev.json','utf8')) as {origin:string;email:string;password:string};
 if(credentials.origin!==origin)throw new Error('Development sign-in origin mismatch');
 await page.getByLabel('Work email',{exact:true}).fill(credentials.email);
 await page.getByLabel('Password',{exact:true}).fill(credentials.password);
 await page.getByRole('button',{name:'Sign in',exact:true}).click();
 await expect(page.getByRole('button',{name:'Switch workspace',exact:true})).toBeEnabled({timeout:60_000});
}
async function ready(page:Page){await expect(page.getByRole('button',{name:'Switch workspace',exact:true})).toBeEnabled({timeout:60_000});await expect(page.locator('main')).not.toContainText('Loading your workspace…',{timeout:60_000});}
test.beforeEach(async({page,baseURL})=>{
 if(baseURL!==origin)throw new Error('This read-only route check requires the authorized development console');
 await page.goto(origin+'/login?next='+encodeURIComponent(base+'/administration/technology/directory/connections'));
 await page.getByLabel('Work email',{exact:true}).waitFor({state:'visible',timeout:60_000});await signIn(page);
 await ready(page);
});
test('workspace shell hydrates before its topic controls become interactive',async({page},info)=>{
 const errors:string[]=[];page.on('pageerror',error=>errors.push(error.message));
 await expect(page.getByRole('heading',{name:'Directory',exact:true}).first()).toBeVisible();
 await expect(page.getByRole('navigation',{name:'Directory sections',exact:true}).getByRole('link',{name:'Connections',exact:true})).toHaveAttribute('aria-current','page');
 await page.getByRole('button',{name:'Show breadcrumb path',exact:true}).click();
 await expect(page.getByRole('group',{name:'Breadcrumb path'})).toContainText('Administration');
 await expect(page.getByRole('group',{name:'Breadcrumb path'})).toContainText('Technology');
 await page.keyboard.press('Escape');
 await page.screenshot({path:info.outputPath('connections.png'),fullPage:false});
 expect(errors).toEqual([]);
});
test('combined workspaces render every owner section in both themes without business mutations',async({page,context},info)=>{
 const writes:string[]=[],errors:string[]=[];page.on('request',r=>{if(['POST','PUT','PATCH','DELETE'].includes(r.method())&&r.url().includes('/api/v1/holdings/'))writes.push(new URL(r.url()).pathname);});page.on('pageerror',e=>errors.push(e.message));
 const routes=[['work/actions','My work'],['work/tracking','My work'],['calendar/coverage','Calendar'],['inbox/teams','Inbox'],['people-organization/people/hr-settings/leave','HR settings'],['customers-sales/sales/documents/quote','Sales'],['purchasing/orders','Purchase orders'],['inventory-quality/products','Products'],['projects-routines/projects','Projects'],['services/assets/register','Assets & maintenance'],['finance/setup/books','Finance setup'],['administration/technology/resources/accounts','Resources']] as const;
 for(const theme of ['light','dark']){
  await context.addCookies([{name:'erpx_appearance',value:theme,url:origin}]);
  for(const [route] of routes)await test.step(theme+': '+route,async()=>{await page.goto(origin+base+'/'+route);await ready(page);await expect(page.locator('html')).toHaveAttribute('data-theme',theme);await expect(page.locator('main h1').first()).toBeVisible();await expect(page.locator('main')).not.toContainText('Something went wrong');expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);await page.reload();await ready(page);});
 }
 await page.screenshot({path:info.outputPath('submenu-dark.png')});expect(writes).toEqual([]);expect(errors).toEqual([]);
});
test('path records preserve repeated collection filters and selected detail tabs',async({page,context})=>{
 await page.goto(origin+base+'/people-organization/people/directory/current?q=Tara&filter=one&filter=two');await ready(page);
 await page.getByRole('button',{name:'Tara Suwan',exact:true}).click();
 await expect(page).toHaveURL(/\/directory\/person\/[^/]+\/profile/,{timeout:60_000});
 const detail=page.getByRole('complementary',{name:'Tara Suwan details',exact:true});await expect(detail).toBeVisible({timeout:60_000});
 const employment=detail.getByRole('link',{name:'Employment',exact:true});const href=await employment.getAttribute('href');expect(href).toMatch(/\/employment\?/);
 await employment.click();await expect(page).toHaveURL(/\/employment\?/);await expect(detail).toBeVisible({timeout:60_000});
 await page.reload();await ready(page);await expect(detail).toBeVisible({timeout:60_000});
 const newTab=await context.newPage();await newTab.goto(origin+href!);await ready(newTab);await expect(newTab.getByRole('complementary',{name:'Tara Suwan details',exact:true})).toBeVisible({timeout:60_000});await newTab.close();
 await page.goBack();await expect(page).toHaveURL(/\/profile\?/);await expect(detail).toBeVisible({timeout:60_000});await page.goForward();await expect(page).toHaveURL(/\/employment\?/);
 await detail.getByRole('button',{name:'Close details',exact:true}).click();await expect(detail).toHaveCount(0,{timeout:60_000});const closed=new URL(page.url());expect(closed.searchParams.getAll('filter')).toEqual(['one','two']);expect(closed.searchParams.get('q')).toBe('Tara');
});
test('signed-out deep links return through normal sign-in to the same detail path',async({browser})=>{
 const context=await browser.newContext();const page=await context.newPage();
 const path=base+'/people-organization/people/directory/person/demo-org-v1-bangkok-accounting-staff-engagement/employment';
 await page.goto(origin+path);await expect(page.getByLabel('Work email',{exact:true})).toBeVisible({timeout:60_000});
 expect(new URL(page.url()).searchParams.get('next')).toBe(path);await signIn(page);await expect(page).toHaveURL(origin+path);await expect(page.getByRole('complementary',{name:'Tara Suwan details',exact:true})).toBeVisible({timeout:60_000});await context.close();
});
test('missing record IDs recover without command writes',async({page})=>{
 const writes:string[]=[];page.on('request',r=>{if(r.method()!=='GET'&&r.url().includes('/api/v1/holdings/'))writes.push(new URL(r.url()).pathname);});
 await page.goto(origin+base+'/projects-routines/projects/missing-route-verification/overview');await ready(page);
 await expect(page.getByTestId('record-detail-drawer')).toBeVisible({timeout:60_000});await expect(page.locator('main')).toContainText(/unavailable|not found|cannot/i,{timeout:60_000});expect(writes).toEqual([]);
});
