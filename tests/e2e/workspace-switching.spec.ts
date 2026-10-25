import { captureCaseScreenshot } from "../helpers/case-screenshot";
import { test, expect } from '@playwright/test';
import {openAccountMenu,matchingRoleOptions,selectRoleForSetup} from '../helpers/context-switching-browser';

test.setTimeout(180_000);
test.use({actionTimeout:30_000,navigationTimeout:60_000});

test('switches areas and preserves roles', async ({ page, context }) => {
  expect((await context.request.post('/api/auth/sign-in/email', { data: {
    email: process.env.ERPX_E2E_ADMIN_EMAIL ?? 'jesadakorn.kirtnu@gmail.com',
    password: process.env.ERPX_E2E_ADMIN_PASSWORD ?? '12345678',
  } })).ok()).toBe(true);
  expect((await context.request.post('/api/v1/holdings/acme/companies/active',{data:{companyId:'acme-co'}})).ok()).toBe(true);
  await selectRoleForSetup(context,'holding_owner');
  await page.goto('/platform/home');
  await expect(page).toHaveURL(/\/holdings\/erpx\/manage-erpx$/);
  await expect(page.getByRole('heading', {name:'ERPX Management',exact:true})).toBeVisible();
  const switcher=page.getByRole('button',{name:'Switch workspace',exact:true});
  await switcher.click();
  const dialog=page.getByRole('dialog',{name:'Switch workspace'});
  await expect(dialog.getByRole('link',{name:/^ERPX Management(?: Current area)?$/})).toHaveAttribute('aria-current','location');
  await page.keyboard.press('Escape');
  await expect(switcher).toBeFocused();
  await switcher.press('Enter');
  await Promise.all([
    page.waitForResponse(r=>r.url().endsWith('/companies/active') && r.request().postDataJSON().companyId==='acme-co' && r.ok()),
    dialog.getByRole('button',{name:'Acme Corporation',exact:true}).click(),
  ]);
  await expect(page.locator('[data-work-revision]')).toBeVisible();
  for(const [companyId,companyName] of [['beacon-co','Beacon Labs'],['acme-co','Acme Corporation']] as const) {
    await switcher.click();
    await Promise.all([
      page.waitForResponse(r=>r.url().endsWith('/companies/active') && r.request().postDataJSON().companyId===companyId && r.ok()),
      dialog.getByRole('button',{name:companyName,exact:true}).click(),
    ]);
    await expect(page.locator('[data-work-company]')).toHaveAttribute('data-work-company',companyId);
    await openAccountMenu(page);
    const roleOptions=await matchingRoleOptions(page,'');
    expect(await roleOptions.count()).toBeGreaterThanOrEqual(17);
    const employeeOptions=await matchingRoleOptions(page,'employee');
    await employeeOptions.first().click();
    await expect(page.getByRole('button',{name:/Account menu for/})).toContainText(/employee/i);
  }
  await switcher.click();
  const bangkok=dialog.locator('a[href="/holdings/acme/branches/bangkok/home"]');
  await bangkok.click();
  await expect(page).toHaveURL(/\/holdings\/acme\/branches\/bangkok\/home$/);
  await switcher.click();
  await expect(bangkok).toHaveAttribute('aria-current','location');
  await page.keyboard.press('Escape');
  await switcher.click();
  await dialog.getByRole('link',{name:/^ERPX Management(?: Current area)?$/}).click();
  await expect(page).toHaveURL(/\/holdings\/erpx\/manage-erpx$/);
  await expect(page.getByRole('heading',{name:'ERPX Management',exact:true})).toBeVisible();
  await switcher.click();
  await Promise.all([page.waitForResponse(r=>r.url().endsWith('/workspace-preference') && r.request().postDataJSON().base==='/holdings/acme' && r.ok()),dialog.getByRole('button',{name:/Acme Corporation/}).click()]);
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await page.goto('/');await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await switcher.click();await dialog.getByRole('button',{name:'Northstar Supply',exact:true}).click();
  await expect(page).toHaveURL(/\/(work-role\?|holdings\/northstar\/home)/);
  await expect(page.getByText('You do not have access',{exact:false})).toHaveCount(0);
});

test('retired Platform catalogs stay unavailable and out of search',async({page,context})=>{
  expect((await context.request.post('/api/auth/sign-in/email',{data:{
    email:process.env.ERPX_E2E_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com',
    password:process.env.ERPX_E2E_ADMIN_PASSWORD??'12345678',
  }})).ok()).toBe(true);
  for(const route of ['/platform/modules','/platform/modules/finance/ledger']){
    const response=await page.request.get(route,{maxRedirects:0});
    expect(response.status(),route).toBe(404);
    expect(response.headers().location,route).toBeUndefined();
  }
  await page.goto('/platform/home');
  await page.getByRole('button',{name:'Search workspace'}).click();
  await page.getByRole('dialog',{name:'Go to…'}).getByRole('textbox').fill('module');
  await expect(page.getByRole('dialog',{name:'Go to…'}).getByRole('link')).toHaveCount(0);
});

test('business-only staff sees its holding without Platform access',async({page,context,baseURL})=>{
  test.skip(!['localhost','127.0.0.1'].includes(new URL(baseURL!).hostname),'Local seeded staff account');
  expect((await context.request.post('/api/auth/sign-in/email',{data:{email:process.env.ERPX_E2E_READER_EMAIL??'elena.suwan@erpx.dev',password:process.env.ERPX_E2E_READER_PASSWORD??'12345678'}})).ok()).toBe(true);
  await page.goto('/holdings/acme/branches/bangkok/departments/operations/home');
  await page.getByRole('button',{name:'Switch workspace',exact:true}).click();
  const dialog=page.getByRole('dialog',{name:'Switch workspace'});
  await expect(dialog.locator('a[href="/holdings/acme/branches/bangkok/departments/operations/home"]')).toContainText('Operations');
  await expect(dialog.getByRole('link',{name:/^ERPX Management(?: Current area)?$/})).toHaveCount(0);
  await expect(dialog.locator('a[href="/holdings/erpx/manage-erpx"]')).toHaveCount(0);
  await expect(dialog.getByRole('link',{name:/Northstar/})).toHaveCount(0);
  await expect(dialog.getByRole('button',{name:'Northstar Supply',exact:true})).toHaveCount(0);
});

test('failed company selection leaves the active company usable, owner can open finance',async({page,context},testInfo)=>{
  expect((await context.request.post('/api/auth/sign-in/email',{data:{email:process.env.ERPX_E2E_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com',password:process.env.ERPX_E2E_ADMIN_PASSWORD??'12345678'}})).ok()).toBe(true);
  expect((await context.request.post('/api/v1/holdings/acme/companies/active',{data:{companyId:'acme-co'}})).ok()).toBe(true);
  await selectRoleForSetup(context,'employee');
  await page.goto('/holdings/acme/home');
  await expect(page.getByRole('button',{name:'Switch workspace',exact:true})).toBeEnabled();
  await page.route('**/companies/active',route=>route.fulfill({status:403,contentType:'application/json',body:JSON.stringify({error:{message:'Unavailable'}})}));
  await page.getByRole('button',{name:'Switch workspace',exact:true}).click();
  const company=page.getByRole('button',{name:'Beacon Labs',exact:true});
  await company.click();
  await expect(page.getByRole('dialog').getByRole('alert')).toContainText('Business could not be selected');
  await expect(company).toBeEnabled();
  await expect(page.getByRole('button',{name:/Acme Corporation/})).toHaveAttribute('aria-current','location');
  await page.unroute('**/companies/active');
  await page.keyboard.press('Escape');
  await openAccountMenu(page);
  const roles=await matchingRoleOptions(page,'holding owner');
  await roles.click();
  await expect(page.locator('.active-work-role')).toContainText('holding owner',{timeout:30_000});
  await expect(page).toHaveURL(/\/holdings\/acme\/home$/);
  await captureCaseScreenshot(page, `workspace-${testInfo.project.name}.png`);
  await page.goto('/holdings/acme/finance');
  await expect(page.locator('[data-work-revision]')).toBeVisible();
  expect((await context.request.get('/api/v1/holdings/acme/finance/charts')).status()).toBe(200);
  await page.goto('/holdings/acme/branches/bangkok/home');
  await page.getByRole('button',{name:'App launcher',exact:true}).click();
  const navigation = page.getByRole('dialog',{name:'App launcher',exact:true});
  await expect(navigation.locator('a[href="/holdings/acme/branches/bangkok/people"]')).toBeVisible();
  for(const destination of ['people/payroll','settings?section=approvals','settings?section=usage','settings?section=ai'])
    await expect(navigation.locator(`a[href="/holdings/acme/branches/bangkok/${destination}"]`)).toHaveCount(0);
  await page.keyboard.press('Escape');
  await page.goto('/holdings/acme/branches/bangkok/settings?section=usage');
  await expect(page).toHaveURL(/\/holdings\/acme\/branches\/bangkok\/settings\?section=usage$/);
  await expect(page.getByRole('heading',{name:'Your role cannot open this view',exact:true})).toBeVisible();
});
