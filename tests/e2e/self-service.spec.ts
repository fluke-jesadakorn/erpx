import { test, expect, type Page } from '@playwright/test';
import { randomUUID } from 'node:crypto';
const base='/holdings/acme';
test.setTimeout(120_000);
test.use({actionTimeout:20_000});
test.beforeEach(async({context})=>{
 const response=await context.request.post('/api/auth/sign-in/email',{data:{email:'mali@erpx.dev',password:'Mali-ERPX-2026!'}});
 expect(response.status()).toBe(200);
});
async function visit(page:Page,path:string){
 await page.goto(`${base}${path}`);
 if(page.url().includes('/work-role')){
  await page.getByRole('combobox',{name:'Role for actions & approvals'}).fill('people manager');
  await page.getByRole('listbox').getByRole('option').first().click();
  await page.goto(`${base}${path}`);
 }
}
async function fits(page:Page){expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);}

test('Home keeps its feed, shortcuts, keyboard access and independent failure recovery',async({page},info)=>{
 await page.route('**/self-service/summary',route=>route.fulfill({status:503,contentType:'application/json',body:JSON.stringify({error:{message:'My day is temporarily unavailable.'}})}));
 await visit(page,'/home');
 const shortcuts=page.getByRole('navigation',{name:'Staff self-service'});
 await expect(shortcuts.getByRole('link')).toHaveCount(4);
 await expect(shortcuts.getByRole('link',{name:/My shifts/})).toHaveAttribute('href',`${base}/people/roster?view=mine`);
 await expect(page.getByRole('region',{name:'Team updates'})).toBeVisible();
 const day=page.locator('aside').filter({has:page.locator('summary').filter({hasText:'My day'})});
 if(info.project.name==='mobile'){
  await expect(day.locator('details')).not.toHaveAttribute('open');
  await day.locator('summary').press('Enter');
 }
 await expect(day.getByRole('alert')).toContainText('temporarily unavailable');
 await page.getByRole('button',{name:'Following',exact:true}).click();
 await expect(page.getByRole('button',{name:'Following',exact:true})).toHaveAttribute('aria-pressed','true');
 await page.getByRole('button',{name:'Latest updates',exact:true}).click();
 const article=page.getByRole('article').first();
 const thanks=article.getByRole('button',{name:/^Thanks/});
 const initialReaction=await thanks.getAttribute('aria-pressed');await thanks.click();await expect(thanks).toHaveAttribute('aria-pressed',initialReaction==='true'?'false':'true');await thanks.click();await expect(thanks).toHaveAttribute('aria-pressed',initialReaction!);
 await article.getByRole('button',{name:/^Comment/}).click();await expect(article.getByLabel('Write a comment',{exact:true})).toBeVisible();await article.getByRole('button',{name:/^Comment/}).click();
 await article.getByLabel('More post actions').click();const follow=article.getByRole('button',{name:/^(Unfollow|Follow) conversation$/});const initialFollow=await follow.getAttribute('aria-pressed');await follow.click();await expect(follow).toHaveAttribute('aria-pressed',initialFollow==='true'?'false':'true');await follow.click();await expect(follow).toHaveAttribute('aria-pressed',initialFollow!);await article.getByLabel('More post actions').click();
 const before=await page.getByRole('article').count();await page.getByRole('button',{name:/Load .*older updates/}).click();await expect.poll(()=>page.getByRole('article').count()).toBeGreaterThan(before);
 await page.getByText('Share an update',{exact:true}).click();
 await expect(page.getByLabel('Post message',{exact:true})).toBeVisible();
 await page.getByLabel('Post message',{exact:true}).fill('Unpublished self-service verification draft');
 await page.getByText('Share an update',{exact:true}).click();
 await page.unroute('**/self-service/summary');
 await day.getByRole('button',{name:'Try again'}).click();
 await expect(day.getByRole('heading',{name:'Your next shift'})).toBeVisible({timeout:45_000});
 await fits(page);
 for(const theme of ['light','dark']){
  await page.evaluate(t=>document.documentElement.setAttribute('data-theme',t),theme);
  await page.screenshot({path:`.scratch/self-service/home-${info.project.name}-${theme}.png`,fullPage:true});
  await fits(page);
 }
 const overtime=shortcuts.getByRole('link',{name:/Overtime/});
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
 const headers={'Idempotency-Key':randomUUID(),'x-erpx-context-revision':(await rendered.getAttribute('data-work-revision'))!,'x-erpx-context-company':(await rendered.getAttribute('data-work-company'))!,'x-erpx-context-assignment':(await rendered.getAttribute('data-work-assignment'))!};
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
 await page.screenshot({path:`.scratch/self-service/overtime-${info.project.name}.png`,fullPage:true});
 await page.getByRole('button',{name:'Withdraw request'}).click();
 await expect(page.getByRole('heading',{name:'Withdrawn',exact:true})).toBeVisible();
});

test('leave draft, half-day preview and interrupted private upload can be recovered',async({page},info)=>{
 await visit(page,'/leave/new');
 const select=page.getByRole('combobox',{name:'Leave type',exact:true});
 await expect(select).toBeVisible();
 await select.selectOption({index:1});
 await page.getByLabel('Start date',{exact:true}).fill('2026-11-09');
 await page.getByLabel('End date',{exact:true}).fill('2026-11-09');
 await page.getByRole('combobox',{name:'Start day',exact:true}).selectOption('first_half');
 await page.getByRole('combobox',{name:'End day',exact:true}).selectOption('first_half');
 await page.getByLabel('Handover note').fill('Synthetic browser verification, private draft.');
 await page.getByRole('button',{name:'Save draft',exact:true}).click();
 await expect(page).toHaveURL(/\/tasks\/people_payroll\/leave-/);
 await page.reload();
 await expect(page.getByRole('combobox',{name:'Start day',exact:true})).toHaveValue('first_half');
 await page.getByRole('button',{name:'Calculate request'}).click();
 await expect(page.getByText('0.5 working days',{exact:true})).toBeVisible();
 let interrupt=true;
 await page.route('**/uploads/**',async route=>{if(route.request().method()==='PUT'&&interrupt){interrupt=false;await route.abort('failed');}else await route.continue();});
 await page.getByLabel('Private supporting evidence',{exact:true}).setInputFiles({name:'self-service-evidence.csv',mimeType:'text/csv',buffer:Buffer.from('test,result\nrecovery,verified\n')});
 const upload=page.getByRole('button',{name:'Upload / retry self-service-evidence.csv'});
 await upload.click();
 await expect(page.getByRole('main').getByRole('alert')).toBeVisible();
 await upload.click();
 await expect(page.getByText('self-service-evidence.csv · uploaded and verified',{exact:true})).toBeVisible();
 await page.reload();
 await expect(page.getByRole('link',{name:/self-service-evidence.csv/})).toBeVisible();
 await fits(page);
 await page.screenshot({path:`.scratch/self-service/leave-${info.project.name}.png`,fullPage:true});
 await page.getByRole('button',{name:'cancel',exact:true}).click();
 await expect(page.getByRole('heading',{name:'cancelled',exact:true})).toBeVisible();
});

test('My shifts and Purchase request retain their canonical destinations',async({page})=>{
 await visit(page,'/people/roster?view=mine');
 await expect(page.getByRole('main')).toContainText(/My shifts|My roster/);
 await fits(page);
 await visit(page,'/work?new=purchase_request');
 await expect(page.getByRole('heading',{name:/Purchase request/i})).toBeVisible();
 await fits(page);
});
