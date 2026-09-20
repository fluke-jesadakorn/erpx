import { expect, test } from '@playwright/test';
const base='/holdings/acme';
test.beforeEach(async({context,baseURL})=>{
 test.skip(!['localhost','127.0.0.1'].includes(new URL(baseURL!).hostname),'Uses the existing local test account.');
 const response=await context.request.post('/api/auth/sign-in/email',{data:{email:'mali@erpx.dev',password:'Mali-ERPX-2026!'}});
 expect(response.status()).toBe(200);
});
test('authenticated features keep one navigation layer in light and dark',async({page,context,baseURL})=>{
 test.setTimeout(180000);
  for(const theme of ['light','dark']){
  await context.addCookies([{name:'erpx_appearance',value:theme,url:baseURL!}]);
  for(const route of ['home','work','inbox','calendar','people']){
   await page.goto(`${base}/${route}`);
   await expect(page.locator('main h1').first()).toBeVisible({timeout:30000});
   const layoutButton=route==='people'?page.getByRole('button',{name:'People tools',exact:true}):page.getByRole('button',{name:'Customize layout',exact:true}).first();
   if(route==='calendar') await expect(layoutButton).toHaveCount(0);
   else await expect(layoutButton).toBeEnabled({timeout:30000});
   expect(await page.locator('main [data-feature-tabs]').count(),route).toBeLessThanOrEqual(1);
   if(route==='people') expect(await page.locator('[data-feature-tabs] a').count()).toBeLessThanOrEqual(4);
   await expect(page.locator('[data-feature-widget] [data-feature-tabs]')).toHaveCount(0);
   expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),route).toBe(true);
  }
 }
});
test('cancel preserves search state and restores widget geometry without business commands',async({page})=>{
 await page.goto(`${base}/people`);
 const current=page.locator('[data-feature-widget="current-profiles"]');
 await expect(page.getByRole('button',{name:'People tools',exact:true})).toBeEnabled({timeout:30000});
 const mutations:string[]=[];page.on('request',r=>{if(r.method()==='POST'&&r.url().includes('/api/v1'))mutations.push(r.url());});
 await page.getByRole('button',{name:'People tools',exact:true}).click();
 await page.getByRole('combobox',{name:'Current profiles width',exact:true}).selectOption('6');
 await page.getByRole('button',{name:'Move Current profiles',exact:true}).press('ArrowDown');
 await page.locator('article').filter({hasText:'Archived profiles'}).getByRole('checkbox',{name:'Visible'}).uncheck();
 page.once('dialog',dialog=>dialog.accept());
 await page.getByRole('button',{name:'Cancel',exact:true}).click();
 await expect(current).toBeVisible();await expect(page.locator('[data-feature-widget="archived-profiles"]')).toBeVisible();
 expect(mutations).toEqual([]);
});
test('People tools exposes secondary views, active state, and responsive presentation',async({page},testInfo)=>{
 await page.goto(`${base}/handbook`);
 await page.getByRole('button',{name:'People tools',exact:true}).click();
 const drawer=page.getByRole('dialog',{name:'People views and layout'});
 await expect(drawer).toBeVisible();
 await expect(drawer.getByRole('link',{name:/Company handbook/})).toHaveAttribute('aria-current','page');
 await expect(drawer.getByRole('link',{name:'Learn'})).toBeVisible();
 await expect(drawer.getByRole('link',{name:'People settings'})).toBeVisible();
 const box=await drawer.boundingBox();
 expect(box).not.toBeNull();
 if(testInfo.project.name==='mobile'){
  expect(box!.width).toBeGreaterThan(page.viewportSize()!.width*.9);
  expect(box!.y).toBeGreaterThan(0);
 }else{
  expect(box!.width).toBeLessThanOrEqual(441);
  expect(box!.x+box!.width).toBeGreaterThanOrEqual(page.viewportSize()!.width-1);
 }
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);
 await page.keyboard.press('Escape');
 await expect(drawer).toBeHidden();
 await expect(page.getByRole('button',{name:'People tools',exact:true})).toBeFocused();
});
