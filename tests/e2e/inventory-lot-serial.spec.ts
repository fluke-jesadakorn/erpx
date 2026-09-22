import {expect,test} from '@playwright/test';

test('inventory screens expose traceability and separate stock positions',async({context,page},testInfo)=>{
  const login=await context.request.post('/api/auth/sign-in/email',{data:{email:process.env.ERPX_E2E_ADMIN_EMAIL??'jesadakorn.kirtnu@gmail.com',password:process.env.ERPX_E2E_ADMIN_PASSWORD??'12345678'}});
  expect(login.ok()).toBe(true);

  const sku=`LOT-${testInfo.project.name.slice(0,3).toUpperCase()}-${Date.now().toString(36)}`;
  await page.goto('/holdings/acme/commerce?view=products&new=1');
  await expect(page.getByRole('heading',{name:'Business operations'})).toBeVisible();
  await page.getByLabel('SKU').fill(sku);
  await page.getByLabel('Name').fill('Browser acceptance lot item');
  await page.getByLabel('Unit').fill('box');
  await page.getByLabel('Traceability').selectOption('lot');
  await page.getByLabel('List price').fill('1200');
  await page.getByLabel('Default tax (%)').fill('7');
  await page.getByRole('button',{name:'Save product'}).click();
  const row=page.getByRole('row').filter({hasText:sku});
  await expect(row).toBeVisible();
  await expect(row).toContainText('lot tracked');

  await page.goto('/holdings/acme/commerce?view=stock');
  for(const header of ['On hand','Reserved','Quarantined','In transit','Available'])await expect(page.getByRole('columnheader',{name:header})).toBeVisible();

  await page.goto('/holdings/acme/quality');
  await expect(page.getByRole('heading',{name:'Quality holds'})).toBeVisible();
  await expect(page.getByLabel('Product, lot / serial, and warehouse')).toBeVisible();
});
