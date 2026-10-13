import { captureCaseScreenshot } from "../../support/helpers/case-screenshot";
import { test, expect, type BrowserContext, type Page } from '@playwright/test';
import { randomUUID } from 'node:crypto';
import { selectFixtureAssignment } from "../../support/browser/select-fixture-assignment";
import { openAccountMenu } from "../../support/helpers/context-switching-browser";
const base='/holdings/acme';
test.setTimeout(120_000);
test.use({actionTimeout:20_000});
test.beforeEach(async({context})=>{
 const response=await context.request.post('/api/auth/sign-in/email',{data:{email:'mali@erpx.dev',password:'Mali-ERPX-2026!'}});
 expect(response.status()).toBe(200);
 await selectFixtureAssignment(context,'acme-co','seed_usr_mali');
});
async function visit(page:Page,path:string){
 await page.goto(`${base}${path}`);
}
async function refreshAfterTabReturn(page:Page,path:string){
 const response=page.waitForResponse(r=>new URL(r.url()).pathname===path&&r.request().method()==='GET');
 const other=await page.context().newPage();
 try { await other.goto('about:blank');await page.bringToFront();return await response; }
 finally { await other.close(); }
}
type SavedLeave={id:string;state:string;version:number;start_segment:string;end_segment:string};
async function readLeave(context:BrowserContext,id:string):Promise<SavedLeave>{
 const response=await context.request.get(`/api/v1${base}/leave-requests/${id}`);
 expect(response.ok(),await response.text()).toBe(true);
 return (await response.json() as {data:SavedLeave}).data;
}
const evidencePng=Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAACXBIWXMAAAPoAAAD6AG1e1JrAAAAEUlEQVQImWOQaEj4D8IMMAYAQaQH3RNNL/AAAAAASUVORK5CYII=','base64');
async function fits(page:Page){expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);}

test('Home keeps its feed, one shared shelf, keyboard access and independent failure recovery',async({page},info)=>{
 await visit(page,'/home');
 await expect(page.getByRole('heading',{name:'Today',exact:true})).toBeVisible();
 const tools=page.getByRole('complementary',{name:'Today tools',exact:true});
 const toggle=page.locator('#today-tools-toggle');
 const compact=await toggle.evaluate(el=>getComputedStyle(el.parentElement!).display!=='none');
 if(compact){
  await expect(toggle).toBeVisible();
  await toggle.focus();await expect(toggle).toBeFocused();await toggle.press('Enter');
  await expect(toggle).toHaveAttribute('aria-expanded','true');
 }else await expect(toggle).toBeHidden();
 await expect(tools).toBeVisible();
 const start=info.project.name==='mobile'?page.getByRole('navigation',{name:'Mobile navigation',exact:true}).getByRole('button',{name:'Start work',exact:true}):page.getByRole('navigation',{name:'Primary',exact:true}).getByRole('button',{name:'Start',exact:true});
 await start.click();
 const startDialog=page.getByRole('dialog',{name:'Start work',exact:true});
 const shortcuts=startDialog.getByRole('region',{name:'Start work',exact:true});
 await expect(shortcuts).toHaveCount(1);
 await expect(page.getByRole('navigation',{name:'Staff self-service'})).toHaveCount(0);
 for(const title of ['Leave request','Request overtime','Purchase request'])await expect(shortcuts.getByRole('link',{name:title,exact:true})).toHaveCount(1);
 await page.keyboard.press('Escape');await expect(start).toBeFocused();
 const feed=page.getByRole('region',{name:'Team updates',exact:true});
 await expect(feed).toBeVisible();
 const day=tools.getByRole('region',{name:'My day',exact:true});
 await expect(day.getByRole('link',{name:'Open Calendar',exact:true})).toBeVisible();
 await page.route('**/self-service/summary',route=>route.fulfill({status:503,json:{error:{message:'My day is temporarily unavailable.'}}}));
 expect((await refreshAfterTabReturn(page,`/api/v1${base}/self-service/summary`)).status()).toBe(503);
 await expect(day.getByRole('alert')).toContainText('temporarily unavailable');
 await page.getByRole('button',{name:'Following',exact:true}).click();
 await expect(page.getByRole('button',{name:'Following',exact:true})).toHaveAttribute('aria-pressed','true');
 await page.getByRole('button',{name:'Latest updates',exact:true}).click();
 const article=feed.getByRole('article').first();
 const thanks=article.getByRole('button',{name:/^Thanks/});
 const initialReaction=await thanks.getAttribute('aria-pressed');await thanks.click();await expect(thanks).toHaveAttribute('aria-pressed',initialReaction==='true'?'false':'true');await thanks.click();await expect(thanks).toHaveAttribute('aria-pressed',initialReaction!);
 await article.getByRole('button',{name:/^Comment/}).click();await expect(article.getByRole('combobox',{name:'Write a comment',exact:true})).toBeVisible();await article.getByRole('button',{name:/^Comment/}).click();
 await article.getByLabel('More post actions').click();const follow=article.getByRole('button',{name:/^(Unfollow|Follow) conversation$/});const initialFollow=await follow.getAttribute('aria-pressed');await follow.click();await expect(follow).toHaveAttribute('aria-pressed',initialFollow==='true'?'false':'true');await follow.click();await expect(follow).toHaveAttribute('aria-pressed',initialFollow!);await article.getByLabel('More post actions').click();
 const before=await feed.getByRole('article').count();await page.getByRole('button',{name:/Load .*older updates/}).click();await expect.poll(()=>feed.getByRole('article').count()).toBeGreaterThan(before);
 const composer=page.locator('details.beta-composer');
 await composer.locator('summary').first().click();await expect(composer).toHaveAttribute('open','');
 const editor=page.getByRole('dialog',{name:'Share an update',exact:true});
 await expect(editor).toBeVisible();
 await expect(editor.getByLabel('Post message',{exact:true})).toBeVisible();
 await editor.getByLabel('Post message',{exact:true}).fill('Unpublished self-service verification draft');
 await editor.getByRole('button',{name:'Close dialog',exact:true}).click();
 const unsaved=page.getByRole('dialog',{name:'Unsaved changes',exact:true});
 await expect(unsaved).toBeVisible();
 await unsaved.getByRole('button',{name:'Close without saving',exact:true}).click();
 await expect(unsaved).toBeHidden();await expect(editor).toBeHidden();
 await page.unroute('**/self-service/summary');
 const recovered=page.waitForResponse(r=>new URL(r.url()).pathname===`/api/v1${base}/self-service/summary`&&r.status()===200);
 await day.getByRole('button',{name:'Try again',exact:true}).click();await recovered;
 await expect(day.getByRole('alert')).toHaveCount(0);
 await expect(day.getByText('No upcoming published shift.')).toHaveCount(0);
 await fits(page);
 for(const theme of ['Light','Dark']){
  await openAccountMenu(page);
  await page.getByRole('group',{name:'Appearance',exact:true}).getByRole('button',{name:theme,exact:true}).click();await page.keyboard.press('Escape');
  await expect(page.locator('html')).toHaveAttribute('data-theme',theme.toLowerCase());
  await captureCaseScreenshot(page, `home-${info.project.name}-${theme.toLowerCase()}.png`, { fullPage:true });await fits(page);
 }
 await start.click();await expect(startDialog).toBeVisible();
 const overtime=shortcuts.getByRole('link',{name:'Request overtime',exact:true});
 await overtime.focus();await expect(overtime).toBeFocused();await overtime.press('Enter');
 await expect(page.getByRole('heading',{name:'Overtime',exact:true})).toBeVisible();
});

test('overtime server drafts survive reload, retain stale edits, preview and withdraw',async({page,context},info)=>{
 await visit(page,'/overtime/new');
 await page.getByLabel('Request path').selectOption('retrospective');
 await page.getByLabel('Actual start').fill('2026-09-10T22:00');
 await page.getByLabel('Actual end').fill('2026-09-11T02:00');
 await page.getByLabel('Unpaid break (minutes)').fill('30');
 const reason=`Self-service browser verification ${randomUUID().slice(0,8)}`;
 await page.getByRole('textbox',{name:'Reason',exact:true}).fill(reason);
 await page.getByLabel('Late-submission explanation').fill('Synthetic test of overnight draft recovery.');
 await page.getByRole('button',{name:'Review calculated hours'}).click();
 await expect(page.getByText('3.5 hours',{exact:true})).toBeVisible();
 await page.getByRole('button',{name:'Save draft',exact:true}).click();
 await expect(page).toHaveURL(/\/overtime\/ot-/);
 const id=page.url().split('/').pop()!;
 await page.reload();
 await expect(page.getByRole('textbox',{name:'Reason',exact:true})).toHaveValue(reason);
 const data=(await (await context.request.get(`/api/v1${base}/overtime-requests/${id}`)).json()).data;
 const rendered=page.locator('[data-work-revision]');
 const headers={Origin:new URL(info.project.use.baseURL as string).origin,'Idempotency-Key':randomUUID(),'x-erpx-context-revision':(await rendered.getAttribute('data-work-revision'))!,'x-erpx-context-company':(await rendered.getAttribute('data-work-company'))!,'x-erpx-context-assignment':(await rendered.getAttribute('data-work-assignment'))!};
 const changed=await context.request.post(`/api/v1${base}/overtime-requests/${id}`,{headers,data:{action:'save',expectedVersion:data.version,details:{...data.details,reason:reason+' from another tab'}}});
 expect(changed.status(),await changed.text()).toBe(200);
 await page.getByRole('textbox',{name:'Reason',exact:true}).fill(reason+' retained edit');
 await page.getByRole('button',{name:'Save draft',exact:true}).click();
 await expect(page.getByRole('main').getByRole('alert')).toContainText('changed');
 await expect(page.getByRole('textbox',{name:'Reason',exact:true})).toHaveValue(reason+' retained edit');
 await page.getByRole('button',{name:'Load latest version and keep edits'}).click();
 await page.getByRole('button',{name:'Review calculated hours'}).click();
 await expect(page.getByText('3.5 hours',{exact:true})).toBeVisible();
 await fits(page);
 await captureCaseScreenshot(page, `overtime-${info.project.name}.png`, { fullPage:true });
 await page.getByRole('button',{name:'Withdraw request'}).click();
 await expect(page.getByRole('heading',{name:'Withdrawn',exact:true})).toBeVisible();
});

test('leave draft, half-day preview and interrupted private upload can be recovered',async({page,context},info)=>{
 test.setTimeout(240_000);
 await visit(page,'/leave/new');
 const select=page.getByRole('combobox',{name:'Leave type',exact:true});
 await expect(select).toBeVisible();await select.selectOption({index:1});
 await page.getByLabel(/^First day(?:\s*\*)?$/).fill('2026-11-09');
 await page.getByLabel(/^Last day(?:\s*\*)?$/).fill('2026-11-09');
 const portions=page.getByRole('combobox',{name:'Portion',exact:true});
 await expect(portions).toHaveCount(2);await portions.nth(0).selectOption('first_half');await portions.nth(1).selectOption('first_half');
 await page.getByRole('textbox',{name:/^Handover note/}).fill('Synthetic browser verification, private draft.');
 await page.getByRole('button',{name:'Save draft',exact:true}).click();
 await expect(page).toHaveURL(/\/tasks\/people_payroll\/leave-/);
 const id=new URL(page.url()).pathname.split('/').pop()!;
 const draft=await readLeave(context,id);expect(draft).toMatchObject({state:'draft',version:1,start_segment:'first_half',end_segment:'first_half'});
 await page.reload();await expect(portions.nth(0)).toHaveValue('first_half');await expect(portions.nth(1)).toHaveValue('first_half');
 await page.getByRole('button',{name:'Review request',exact:true}).click();
 await expect(page.getByRole('heading',{name:'Check the calculation',exact:true})).toBeVisible();
 await expect(page.getByText('0.5 working days',{exact:true})).toBeVisible();
 await page.getByRole('tab',{name:'Evidence',exact:true}).click();
 const evidence=page.getByRole('heading',{name:'Supporting photos and documents',exact:true}).locator('xpath=ancestor::section[1]');
 let interrupted=0;
 await page.route('**/uploads/**',async route=>{if(route.request().method()==='PUT'&&interrupted===0){interrupted++;await route.abort('failed');}else await route.continue();});
 await evidence.locator('input[type="file"]:not([capture])').setInputFiles({name:'self-service-evidence.png',mimeType:'image/png',buffer:evidencePng});
 const retry=evidence.getByRole('button',{name:'Retry upload for self-service-evidence.png',exact:true});
 await expect(retry).toBeVisible();expect(interrupted).toBe(1);await retry.click();
 await expect(evidence.getByText('Ready to attach',{exact:true})).toBeVisible({timeout:120_000});
 const binding=page.waitForResponse(r=>/\/attachment-sets\/[^/]+\/bind$/.test(new URL(r.url()).pathname)&&r.request().method()==='POST');
 await evidence.getByRole('button',{name:'Attach clean files to this record',exact:true}).click();
 const bound=await binding;expect(bound.ok(),await bound.text()).toBe(true);expect(bound.request().postDataJSON()).toMatchObject({expectedTargetVersion:draft.version});expect((await bound.json()).data).toMatchObject({version:draft.version+1});
 await expect(evidence.getByText('Attached and locked',{exact:true})).toBeVisible();await page.reload();
 const download=evidence.getByRole('link',{name:'Download self-service-evidence.png',exact:true});await expect(download).toBeVisible();
 const file=await context.request.get((await download.getAttribute('href'))!);expect(file.ok()).toBe(true);expect(file.headers()['cache-control']).toBe('private, no-store');expect(file.headers()['content-disposition']).toContain('attachment');expect((await file.body()).byteLength).toBeGreaterThan(0);
 expect(await readLeave(context,id)).toMatchObject({state:'draft',version:draft.version+1});
 await fits(page);await captureCaseScreenshot(page, `leave-${info.project.name}.png`, { fullPage:true });
 await page.getByRole('tab',{name:'Overview',exact:true}).click();
 const cancellation=page.waitForResponse(r=>new URL(r.url()).pathname===`/api/v1${base}/leave-requests/${id}/transition`&&r.request().method()==='POST');
 await page.getByRole('button',{name:'Cancel request',exact:true}).click();
 const cancelled=await cancellation;expect(cancelled.ok(),await cancelled.text()).toBe(true);expect(cancelled.request().postDataJSON()).toMatchObject({action:'cancel',expectedVersion:draft.version+1});
 expect(await readLeave(context,id)).toMatchObject({state:'cancelled',version:draft.version+2});
 await expect(page.getByText('This request is closed. Its decision history is retained below.',{exact:true})).toBeVisible();
});

test('Backoffice roster and Purchase request retain their canonical destinations',async({page})=>{
 await visit(page,'/people/roster?view=mine');
 await expect(page.getByRole('main')).not.toContainText('My shifts');
 await fits(page);
 await visit(page,'/work?new=purchase_request');
 await expect(page.getByRole('heading',{name:/Purchase request/i})).toBeVisible();
 await fits(page);
});
